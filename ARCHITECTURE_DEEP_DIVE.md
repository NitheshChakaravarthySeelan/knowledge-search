# KnowledgeSearch — Architecture Deep Dive

> A thorough analysis of data flow, architectural decisions, issues, and improvements compared to industry best practices (Pinecone, Glean, Algolia, Elasticsearch).

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Data Flow: Ingestion](#2-data-flow-ingestion)
3. [Data Flow: Search & Retrieval](#3-data-flow-search--retrieval)
4. [Data Flow: Q&A (RAG)](#4-data-flow-qa-rag)
5. [Data Flow: MCP Server](#5-data-flow-mcp-server)
6. [Data Flow: Sync Worker (Notion)](#6-data-flow-sync-worker-notion)
7. [Issues with the Current Implementation](#7-issues-with-the-current-implementation)
8. [Recommended Improvements](#8-recommended-improvements)
9. [Comparison with Industry Best Practices](#9-comparison-with-industry-best-practices)
10. [Appendix: Key Configuration Values](#10-appendix-key-configuration-values)

---

## 1. System Overview

KnowledgeSearch is a **hybrid RAG engine** composed of 10 Rust library crates, 3 Rust service binaries, a TypeScript API gateway, and a Next.js frontend. It stores data in Qdrant (vector DB) and PostgreSQL 16 (relational + knowledge graph).

```
                    ┌──────────────────────┐
                    │   Next.js Frontend    │
                    │   (:3000, dark UI)    │
                    └────────┬─────────────┘
                             │ HTTP / SSE
                    ┌────────▼─────────────┐
                    │  Bun/Elysia Gateway  │
                    │  (:8000, proxy)      │
                    └──┬───────┬──────────┘
                       │       │
              ┌────────▼──┐ ┌──▼───────────┐
              │  Search   │ │  Agent Core  │
              │  Worker   │ │  (:8001)     │
              │  (:8081)  │ │  Rig+Gemini  │
              └─────┬─────┘ │  SSE stream  │
                    │       └──────┬───────┘
              ┌─────▼─────┐       │
              │ Hybrid    │       │
              │ Retriever │       │
              └─────┬─────┘       │
                    │             │
        ┌───────────▼─────────────▼─────┐
        │         Qdrant Vector DB      │
        │  Collection: "knowledge_base" │
        │  (dense-text 1024d + sparse)  │
        └───────────────────────────────┘

        ┌───────────────────────────────────┐
        │    Ingestion Worker (poll loop)    │
        │  1. Poll document_jobs (pending)   │
        │  2. Parse via Docling/DOCX/TXT     │
        │  3. SHA-256 change detection       │
        │  4. Postgres graph upsert          │
        │  5. Hierarchical chunking          │
        │  6. Entity annotation              │
        │  7. Dense + sparse embedding       │
        │  8. Qdrant upsert                  │
        └───────────────────────────────────┘

        ┌───────────────────────────────────┐
        │    Sync Worker (Notion polling)    │
        │  Polls Notion API every 30s        │
        │  (ingestion is a placeholder)      │
        └───────────────────────────────────┘

        ┌───────────────────────────────────┐
        │    MCP Server (stdio transport)   │
        │  Tools: search, ingest_pdf        │
        │  Used by Claude/Cursor/VSCode     │
        └───────────────────────────────────┘
```

### Storage Layout

| Store | Data | Access Pattern |
|-------|------|---------------|
| **Qdrant** | Document chunks with dense (1024d) + sparse vectors | Approximate nearest neighbor (ANN) search |
| **PostgreSQL: `document_jobs`** | Job metadata, progress, file path (dedup key) | PK lookup, status polling |
| **PostgreSQL: `kb_nodes`** | Knowledge graph nodes (Document, Class, Function, Placeholder) | ILIKE entity lookup, parent traversal |
| **PostgreSQL: `kb_graph_edges`** | Relations between nodes (IMPORTS, CALLS, DEFINES, REFERENCES, IMPLEMENTS) | Graph traversal (1-2 hops) |

---

## 2. Data Flow: Ingestion

### Step-by-step walkthrough

```
User uploads file via Web UI
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. API Gateway (apps/api/src/main.ts)                       │
│    POST /api/documents                                      │
│    - Receives file (base64) or raw text + title + extension │
│    - Inserts into PostgreSQL document_jobs (status: pending)│
│    - Returns { id, status: "pending" }                      │
└─────────────────────────────────────────────────────────────┘
        │
        ▼ (polling loop, every 2s)
┌─────────────────────────────────────────────────────────────┐
│ 2. Ingestion Worker (services/ingestion-worker/src/main.rs) │
│    - SELECT * FROM document_jobs WHERE status = 'pending'   │
│    - Updates status → 'processing'                          │
│    - Decodes base64 if binary (PDF/DOCX)                    │
│    - Routes to ParserRegistry based on file_extension       │
└─────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. Parsing (crates/documents/src/parsers/parser.rs)         │
│    Path by extension:                                       │
│    .pdf  → DoclingParser: spawns `python scripts/ingest.py` │
│            (subprocess, reads stdout for markdown output)   │
│            Fallback: PdfParser via pdf_oxide crate          │
│    .docx → DocxParser: dotext crate, writes to temp file    │
│    .txt  → PlainTextParser: direct UTF-8 conversion         │
│    .md   → PlainTextParser: direct UTF-8 conversion         │
│    Output: String of extracted text                         │
└─────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. Ingestion Pipeline: process_job()                        │
│    (services/ingestion-worker/src/pipeline.rs)              │
│                                                             │
│    Stage 1: Content Hash                                    │
│    ─────────────────────                                    │
│    - Computes SHA-256 of parsed content                     │
│    - Used for change detection in next stage                │
│                                                             │
│    Stage 2: Postgres Graph Upsert (atomic transaction)      │
│    ────────────────────────────────────────────────────     │
│    a. Hash comparison:                                      │
│       - Looks up existing node by tenant_id + file_path     │
│       - If content_hash matches → skip (no changes)         │
│       - If different → cascade delete old subgraph          │
│         (edges + child nodes)                               │
│    b. Placeholder promotion:                                │
│       - If a Placeholder node exists for this file_path,    │
│         reuses its UUID (preserving all back-links)         │
│    c. Document node upsert:                                 │
│       - Inserts/updates kb_nodes row with                   │
│         node_type = 'Document', content_hash, metadata      │
│    d. AST extraction (graph_extractor.rs):                  │
│       - For .rs, .py, .js, .ts, .md files:                 │
│         extracts classes, functions, imports, relations     │
│       - Imports → REFERENCES edges                          │
│       - Function calls → CALLS edges                        │
│       - Class definitions → DEFINES edges                   │
│       - Trait impls → IMPLEMENTS edges                      │
│    e. Edge insertion with self-healing:                     │
│       - For each edge, if target node doesn't exist:        │
│         creates a Placeholder node so the link survives     │
│                                                             │
│    Stage 3: Hierarchical Chunking → Qdrant                  │
│    ────────────────────────────────────────────────────     │
│    a. HierarchicalChunker (hierarchical.rs):                │
│       - Parent chunks: ~1500 chars (MarkdownSplitter)       │
│       - Child chunks: ~300 chars per parent                 │
│       - Each child has parent_content = parent chunk text   │
│    b. Entity annotation (entity_extractor.rs):              │
│       - For code files: extracts symbols + deps via         │
│         GraphExtractor                                      │
│       - For text: regex heuristics for capitalized          │
│         phrases, CamelCase, acronyms, quoted terms          │
│       - Co-occurrence relations within 100 chars            │
│    c. Embedding generation (batches of 50):                 │
│       - Dense: via NvidiaProvider/GeminiProvider/           │
│         OpenAiProvider (configurable priority)              │
│       - Sparse: via LocalHashingSparseEncoder               │
│         (tokenize → hash to vocab index → TF norm + log)   │
│    d. Qdrant upsert (upsert_chunks_hybrid):                 │
│       - Named vectors: dense-text (f32[]), sparse-text      │
│       - Payload: document_id, tenant_id, content,           │
│         parent_content, entities, entity_names, ingested_at │
│                                                             │
│    Stage 4: AST Node Embeddings → Qdrant                    │
│    ────────────────────────────────────────────────────     │
│    - Each extracted node (function/class) is embedded       │
│      as a separate Qdrant point                             │
│    - Enables symbol-level search                            │
│                                                             │
│    Final: Update document_jobs status → 'completed'         │
│           (or 'failed' with error_message)                  │
└─────────────────────────────────────────────────────────────┘
```

### Key Design Decisions in Ingestion

1. **Python subprocess for PDF parsing** — Docling provides high-quality PDF→Markdown conversion but requires a Python runtime. The Rust process spawns a subprocess per PDF.
2. **Change detection via SHA-256** — Full content hash avoids re-embedding when nothing changed, but requires reading all content first (hash computation is O(n)).
3. **Self-healing graph** — Placeholder nodes ensure edges aren't lost when documents are ingested out of order. Unique to this codebase among the tools surveyed.
4. **Hierarchical chunking** — Decouples retrieval unit (small child chunks for precision) from context unit (large parent chunk for LLM comprehension). This is the single most impactful RAG optimization recommended by industry practitioners.
5. **Batch processing** — Uses `process_in_batches()` utility with batch size 50 for embedding API calls, matching the approach recommended by Pinecone and Algolia.

---

## 3. Data Flow: Search & Retrieval

```
User types query in Web UI (or API call)
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│ API Gateway (:8000) POST /api/search                        │
│ - Validates body with TypeBox schema                        │
│ - Forwards to search-worker (:8081) as JSON                 │
│ - Returns { query, results, latency_ms }                    │
└─────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│ Search Worker (services/search-worker/src/main.rs)          │
│ GET /search?q=...&tenant_id=...&limit=...                   │
│ - Parses query parameters                                   │
│ - Calls SearchService.search(tenant_id, query, limit)       │
└─────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│ SearchService (crates/search/src/service.rs)                │
│ 1. Retrieve 2×limit from HybridRetriever                    │
│ 2. Rerank via CohereReranker (or LocalReranker fallback)    │
│ 3. Truncate to limit                                        │
└─────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│ HybridRetriever (crates/search/src/hybrid.rs)               │
│ Runs 4 parallel passes via tokio::join!                     │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Pass 1: Dense Search                                 │    │
│  │ - Embed query via selected embedding provider        │    │
│  │   (NVIDIA/Gemini/OpenAI/mock)                        │    │
│  │ - Search Qdrant dense-text field with tenant filter  │    │
│  │ - Returns ranked results with cosine distance scores  │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Pass 2: Sparse Search                                │    │
│  │ - Encode query via LocalHashingSparseEncoder         │    │
│  │   (lowercase → tokenize → hash → TF normalize)      │    │
│  │ - Search Qdrant sparse-text field with tenant filter │    │
│  │ - Returns ranked results                             │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Pass 3: Entity-Boosted Search                        │    │
│  │ - Extract entities from query via EntityExtractor    │    │
│  │   (regex: capitalized phrases, CamelCase, acronyms)  │    │
│  │ - Embed each entity name as a separate query         │    │
│  │ - Dense search over Qdrant                           │    │
│  │ - For each entity, also filter by entity_names in    │    │
│  │   payload (exact match)                              │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Pass 4: Graph Traversal Search                       │    │
│  │ - Extract entities from query                        │    │
│  │ - Look up matching kb_nodes via GraphClient          │    │
│  │   (ILIKE on title)                                   │    │
│  │ - Traverse kb_graph_edges up to 2 hops               │    │
│  │ - Resolve to Document node UUIDs                     │    │
│  │ - Embed query + search Qdrant filtered by doc IDs    │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│ Fusion: Weighted Reciprocal Rank Fusion (RRF)              │
│ score(d) = Σ w_i / (rrf_k + rank_i(d))                     │
│ - Deduplicates by chunk_id (keeps longest content)         │
│ - Configurable weights: dense=1.0, sparse=1.0,             │
│   entity=0.8, graph=0.6, rrf_k=60.0                        │
└─────────────────────────────────────────────────────────────┘
        │ (top 2×limit results)
        ▼
┌─────────────────────────────────────────────────────────────┐
│ Reranker (CohereReranker / LocalReranker)                   │
│ - Cohere: POST /v1/rerank with model rerank-english-v3.0   │
│ - Local: bigram Jaccard similarity blended 70/30 with       │
│   original RRF score                                        │
│ - Re-sorts results by reranker score                        │
└─────────────────────────────────────────────────────────────┘
        │ (top limit results)
        ▼
┌─────────────────────────────────────────────────────────────┐
│ Response to user                                            │
│ - Each result includes: chunk_id, document_id, content,     │
│   score, metadata (source, entities, ingested_at)           │
│ - If hierarchical chunk: parent_content is returned instead │
│   of child content for better context                       │
└─────────────────────────────────────────────────────────────┘
```

### Key Design Decisions in Search

1. **4 parallel passes** — Dense (semantic), sparse (lexical BM25-equivalent), entity-boosted, and graph-traversal. This is more diverse than typical 2-pass (dense + BM25) hybrid search.
2. **Weighted RRF** — Uses configurable per-pass weights rather than a single `alpha` parameter. More flexible but harder to tune.
3. **Entity extraction at search time** — Unlike Glean which pre-computes entity annotations, KnowledgeSearch extracts entities from the raw query string at search time. This is computationally cheap (regex-based) but misses entity-aware indexing optimizations.
4. **Parent-content expansion** — During search, if a chunk has `parent_content`, that parent context is returned instead of the smaller child chunk. This implements the parent-child retrieval pattern recommended in RAG best practices.

---

## 4. Data Flow: Q&A (RAG)

```
User types question in Web UI chat
        │
        ▼
┌─────────────────────────────────────────────────────────────┐
│ Frontend (ChatThread.tsx)                                   │
│ - Calls askQuestion() in lib/api.ts                         │
│ - Opens ReadableStream reader on fetch response             │
│ - Renders tokens incrementally as they arrive               │
└─────────────────────────────────────────────────────────────┘
        │ HTTP POST (streaming)
        ▼
┌─────────────────────────────────────────────────────────────┐
│ API Gateway (:8000) POST /api/ask                           │
│ - Forwards as SSE to agent-core (:8001) POST /ask           │
│ - Strips SSE framing, forwards text chunks as raw stream    │
└─────────────────────────────────────────────────────────────┘
        │ SSE stream
        ▼
┌─────────────────────────────────────────────────────────────┐
│ Agent Core (crates/agent-core/src/main.rs)                  │
│                                                             │
│ 1. Session Management:                                      │
│    - Looks up session_id in Arc<Mutex<HashMap<>>>           │
│    - Appends user message to session history                │
│    - Builds conversation context preamble from last 10      │
│      Q&A pairs (or 50 total messages if fewer)              │
│                                                             │
│ 2. Agent Setup (Rig framework):                             │
│    - Model: gemma-4-31b-it (via Gemini gRPC)                │
│    - Tool: search_knowledge_base (KnowledgeBaseTool)        │
│    - Preamble: "You are Knowledge-OS..." instructs           │
│      markdown formatting and source citation                │
│                                                             │
│ 3. Execution:                                               │
│    - agent.stream_prompt(context + user_question)           │
│    - Rig agent may call search_knowledge_base tool          │
│      → SearchService.search() → returns JSON results        │
│    - LLM generates answer with context from tool results    │
│    - Streams SSE events: text (tokens), reasoning, final    │
│                                                             │
│ 4. Memory:                                                  │
│    - Stores assistant response in session history           │
│    - Implicit 50-message limit per session                  │
└─────────────────────────────────────────────────────────────┘
        │ SSE events
        ▼
┌─────────────────────────────────────────────────────────────┐
│ API Gateway strips SSE framing → raw text stream            │
│                                                             │
│ Frontend renders tokens incrementally                       │
│ - ChatMessage.tsx shows assistant message                   │
│ - Markdown rendered (code blocks, lists, etc.)              │
└─────────────────────────────────────────────────────────────┘
```

### Key Design Decisions in Q&A

1. **Tool-calling agent** (Rig) — The LLM decides when to search the knowledge base. This differs from a fixed "retrieve-then-generate" RAG pipeline where retrieval always happens. The agent can choose to search multiple times or not at all.
2. **In-memory session storage** — Sessions stored in `Arc<Mutex<HashMap>>`. No persistence. This means session data is lost on restart and doesn't scale across multiple agent-core instances.
3. **SSE streaming** — Real-time token streaming for responsive UX. The gateway strips SSE framing to deliver raw text, which is simpler for the frontend to consume.

---

## 5. Data Flow: MCP Server

```
MCP Client (Claude Code, Cursor, VSCode)
        │
        │ stdio transport (JSON-RPC)
        ▼
┌─────────────────────────────────────────────────────────────┐
│ MCP Server (crates/mcp-server/src/main.rs)                  │
│                                                             │
│ Initialization:                                             │
│ - NvidiaProvider (or mock) → dense embeddings              │
│ - LocalHashingSparseEncoder → sparse vectors                │
│ - QdrantClient → vector DB connection                       │
│ - HybridRetriever → 4-pass retrieval                       │
│ - CohereReranker (or local fallback)                        │
│ - SearchService orchestrator                                │
│                                                             │
│ Tool Registration:                                          │
│ ┌──────────────────────────────────────────────────────┐    │
│ │ Tool: search_knowledge_base                          │    │
│ │ Args: query, tenant_id (default "default"), limit    │    │
│ │       (default 10)                                   │    │
│ │ Action: SearchService.search(...) → JSON results     │    │
│ │ Returns: Formatted markdown with sources              │    │
│ └──────────────────────────────────────────────────────┘    │
│                                                             │
│ ┌──────────────────────────────────────────────────────┐    │
│ │ Tool: ingest_pdf                                     │    │
│ │ Args: file_path (String)                             │    │
│ │ Action: Spawns `python scripts/ingest.py {path}`     │    │
│ │ Returns: Markdown representation of the PDF          │    │
│ └──────────────────────────────────────────────────────┘    │
│                                                             │
│ Stdio Transport:                                            │
│ - Uses rmcp crate with transport-io feature                 │
│ - Reads JSON-RPC from stdin, writes to stdout               │
│ - Capability: tools (list + call)                           │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. Data Flow: Sync Worker (Notion)

```
┌─────────────────────────────────────────────────────────────┐
│ Sync Worker (services/sync-worker/src/main.rs)              │
│ - Runs on a 30-second interval loop                         │
│ - Initializes NotionClient (API token or sandbox mock)      │
│ - fetch_pages() → calls Notion /v1/search API               │
│ - Logs page titles and sync result                          │
│ - ACTUAL INGESTION IS A PLACEHOLDER                         │
│   (output is printed but not fed into ingestion pipeline)   │
└─────────────────────────────────────────────────────────────┘
```

---

## 7. Issues with the Current Implementation

### 7.1 Critical Issues

#### I1. Notion Sync Worker is a No-Op
`services/sync-worker/src/main.rs` fetches Notion pages but never feeds them into the ingestion pipeline. The sync worker prints results to stdout but does not create `document_jobs` entries. This means the Notion connector exists in name only. Compare with Glean, where connectors are the primary ingestion path with permission mirroring, identity resolution, and incremental crawling.

#### I2. Python Subprocess Per PDF is Fragile
`DoclingParser` in `crates/documents/src/parsers/parser.rs` spawns `python scripts/ingest.py` as a subprocess per document. This has several problems:
- **Latency**: Python interpreter startup time (~100-300ms) is paid per document even for small files.
- **Resource isolation**: No timeouts, no memory limits. A malformed PDF can hang the parser indefinitely.
- **Error handling**: Stderr from the subprocess is captured but not deeply inspected. Docling errors are opaque.
- **Scalability**: Under load, spawning many Python processes can exhaust system resources.
- **Portability**: Requires Python + Docling dependencies in the deployment environment alongside Rust binaries.

Industry standard (Unstructured, Azure Document Intelligence, LlamaIndex): run document parsing in a separate container/service with proper lifecycle management, or use a Rust-native PDF parser with adequate quality.

#### I3. In-Memory Session Storage Doesn't Scale
`agent-core/src/main.rs` stores sessions in `Arc<Mutex<HashMap<String, Vec<ChatMessage>>>>`. This is:
- **Lost on restart**: No persistence means all sessions disappear when the service restarts.
- **Single-node only**: Cannot scale horizontally behind a load balancer without sticky sessions (which MCP's 2026-07-28 spec has explicitly removed).
- **Memory unbounded**: No eviction policy beyond the per-session 50-message limit. With enough sessions, OOM is possible.

#### I4. Local Sparse Embeddings are Weak
`LocalHashingSparseEncoder` in `crates/embeddings/src/sparse.rs` is a simple hash-based bag-of-words encoder. It does not implement BM25 (no IDF component, no term frequency saturation, no document length normalization). It does not implement SPLADE (no learned term expansions). This means:
- Common words are weighted as heavily as rare discriminative terms.
- No synonym handling — "car" and "automobile" are completely different sparse tokens.
- No query expansion — the sparse pass cannot match "vehicles" when the query says "cars".
- The sparse retrieval pass likely underperforms compared to a proper BM25 or SPLADE implementation.

Industry standard: BM25 (for exact term matching) + SPLADE (for learned sparse) or at minimum a proper BM25 implementation with IDF statistics.

#### I5. No Evaluation Framework
There is no systematic measurement of retrieval quality. No golden dataset, no Recall@k, no MRR, no NDCG. The closest thing to evaluation is the unit tests in the fusion module (`crates/search/src/fusion.rs`). This is a critical gap because:
- You cannot measure whether changes to chunking, embedding, or retrieval improve or degrade quality.
- Quality drift in production goes undetected until users complain.
- You cannot compare different embedding providers objectively.

Industry standard: RAGAS, TruLens, or custom evaluation harness with 100-200 question-answer pairs with ground-truth document sources. Track ContextPrecision, ContextRecall, AnswerFaithfulness, AnswerRelevancy.

#### I6. Embedding Provider Agnosticism Causes Dimension Mismatch Risk
When switching embedding providers, the Qdrant collection's vector dimension is fixed at creation time. The current logic creates the collection with NVIDIA's dimension (1024), but this is hardcoded:
```rust
qdrant.ensure_collection("knowledge_base", 1024)?;
```
If someone configures only OpenAI (1536d) or Gemini (768d) without NVIDIA available, the mock fallback generates 1024d vectors anyway via `generate_mock_embedding` which always returns 1024 dimensions. However, if an actual provider with different dimensions were used, the collection would need to be recreated. There is no migration path.

### 7.2 Moderate Issues

#### I7. No Batching in Ingestion Worker's Main Loop
The ingestion worker polls one job at a time (`Loop 0..job_ids.len()`), processes it fully, then moves to the next. There is no batch-level parallelism. If 100 documents are uploaded simultaneously, they are processed sequentially. While `process_in_batches` is used within a single document's embedding step, the job-level concurrency is serial.

Industry pattern (Pinecone AWS reference architecture): SQS queue with multiple worker instances consuming in parallel, each processing one message at a time. Horizontal auto-scaling based on queue depth.

#### I8. Graph Extraction Scope is Too Broad
`GraphExtractor` in `crates/documents/src/parsers/graph_extractor.rs` uses regex-based heuristics for code analysis. For Rust:
- Parses `struct`, `enum`, `trait`, `impl`, `fn`, `use` with regex
- Does not handle nested modules, generics, lifetime annotations, procedural macros

For Python/JS/TS: similar regex heuristics.

Production code intelligence tools (sourcegraph, glean code search) use proper language-specific parsers (tree-sitter, rust-analyzer, etc.). Regex-based extraction will miss edge cases and produce noisy or incomplete graphs.

#### I9. No Dedicated Analytics/Telemetry Pipeline
The `services/analytics-worker/` directory is empty. There are no dashboards, no query logging, no performance metrics, no search quality monitoring. While `tracing-subscriber` provides structured logging, there's no aggregation into a metrics system (Prometheus, Grafana, etc.).

Industry standard: Log every search query with latency, number of results, reranker scores. Track p95 latency. Monitor embedding provider error rates. Set up dashboards for recall@k over time.

#### I10. No Rate Limiting or Auth Beyond Basic Proxy
The API gateway has no authentication, rate limiting, or request validation beyond TypeBox schema validation. The search worker and agent-core are directly accessible on their ports (8081, 8001) without any protection.

### 7.3 Minor Issues

#### I11. Empty Overlap Chunker File
`crates/documents/src/chunkers/overlap.rs` exists but is empty. The overlap logic appears to be embedded in `RecursiveTextChunker` and `HierarchicalChunker` via the `text-splitter` crate, but there's no standalone overlap chunker.

#### I12. Empty Metadata Module
`crates/documents/src/models/metadata.rs` is empty. Likely intended for structured metadata extraction but not implemented.

#### I13. No Cleanup for Failed Ingestion Jobs
If ingestion fails midway (e.g., embedding API error after graph upsert), the job status is set to "failed" but no cleanup is performed on the already-upserted graph nodes and Qdrant points. This can leave orphaned data.

#### I14. Hardcoded Ports and Service Discovery
Ports are hardcoded throughout: gateway expects agent-core at localhost:8001, search-worker at localhost:8081. There's no configurable service discovery, no health-check-based routing, no environment-aware configuration beyond the `.env` file.

#### I15. Reranker API Key Handling
If `COHERE_API_KEY` is set but invalid, the Cohere reranker will fail with an API error before falling back to the local reranker. There is no automatic fallback on API error — only on empty key.

#### I16. No Document-Level Deletion from Graph
`DELETE /documents/:id` in the search worker deletes from Qdrant but does not remove `kb_nodes` and `kb_graph_edges` from PostgreSQL. This means deleted documents leave orphaned graph data.

#### I17. MCP Server Duplicates Service Initialization
The MCP server (`crates/mcp-server/src/main.rs`) independently creates its own `SearchService`, `QdrantClient`, embedding provider, etc. This duplicates the initialization in `search-worker` and `ingestion-worker`. Any configuration change must be updated in 3+ places.

#### I18. No Chunking Configuration Exposure
Chunk sizes (1500/300) are hardcoded in `HierarchicalChunker` and `RecursiveTextChunker`. Users cannot configure these per document type or per use case without code changes.

---

## 8. Recommended Improvements

### P0: Critical (Fix First)

| # | Issue | Improvement | Effort |
|---|-------|-------------|--------|
| I2 | Python subprocess | **Containerize Docling**: Run a lightweight Python microservice (FastAPI) that accepts a file and returns markdown. The Rust parser calls this service via HTTP. This isolates failures, enables scaling, and avoids per-document interpreter startup cost. | 2-3 days |
| I4 | Sparse embeddings | **Replace LocalHashingSparseEncoder with BM25**: Implement proper BM25 using IDF statistics from the corpus. The simplest approach: compute BM25 scores in Rust natively using a token-frequency map maintained across tenants. For the vector path: pre-compute BM25 scores for each chunk's terms and encode as a sparse vector. Alternatively, adopt Qdrant's built-in full-text filter + dense combination. | 3-5 days |
| I5 | No evaluation | **Build an evaluation harness**: Create a golden dataset of 50-100 query-document pairs with relevance judgments. Implement a benchmarking binary that computes Recall@k, MRR, NDCG@k across different retrieval configurations. Run this in CI to detect regressions. Use this data to tune chunk sizes, RRF weights, and embedding provider selection. | 3-5 days |
| I1 | Notion no-op | **Complete the Notion connector**: After fetching pages, create `document_jobs` entries in PostgreSQL with `file_extension` set appropriately (e.g., "notion") and the content as the page body. Add a Notion-specific parser if needed. | 1-2 days |
| I3 | Session storage | **Replace in-memory HashMap with Redis/Valkey**: Use a Redis-backed session store. This persists across restarts, scales horizontally, and allows TTL-based eviction. The `rig` agent framework supports custom memory backends. | 2-3 days |

### P1: High Priority

| # | Issue | Improvement | Effort |
|---|-------|-------------|--------|
| I7 | Serial job processing | **Parallel job processing with bounded concurrency**: Use a `tokio::semaphore` to process multiple ingestion jobs concurrently (e.g., 4-8 at a time). Add a configurable `--max-concurrent-jobs` flag. | 1 day |
| I8 | Regex graph extraction | **Use tree-sitter for code parsing**: Replace regex-based extraction with `tree-sitter` bindings for proper AST parsing. This handles nested structures, generics, macros, and edge cases correctly. Tree-sitter has Rust bindings and supports 50+ languages. | 3-5 days |
| I9 | No analytics | **Add query logging and Prometheus metrics**: Log every search query with: query text, latency, result count, fusion weights, reranker scores. Export metrics (request count, p50/p95/p99 latency, error rate) via `metrics` + `metrics-exporter-prometheus`. Create a Grafana dashboard. | 2-3 days |
| I16 | Orphaned graph data | **Cascade document deletion to graph**: When deleting a document, also delete its `kb_nodes` entry (CASCADE will handle edges) and all associated Qdrant points. The search-worker's `DELETE /documents/:id` should call GraphClient to clean up PostgreSQL. | 0.5 day |
| I6 | Dimension mismatch | **Parameterize embedding dimension**: Read vector dimension from config/env rather than hardcoding 1024. Make it a property of the embedding provider selection. Add a migration command to recreate the collection when dimension changes. | 1 day |

### P2: Medium Priority

| # | Issue | Improvement | Effort |
|---|-------|-------------|--------|
| I10 | No auth | **Add API key authentication**: Add a simple API key check (from `X-API-Key` header) on the gateway. Use bearer token auth for the search-worker and agent-core internals. | 1 day |
| I13 | Orphaned data on failure | **Transactional rollback on ingestion failure**: If any stage of ingestion fails, roll back the graph upsert (delete inserted nodes) and clean up Qdrant points. Wrap the pipeline in a session-scoped span for traceability. | 1 day |
| I14 | Hardcoded ports | **Use environment-based service discovery**: Make all inter-service URLs configurable via environment variables. Default to localhost for development but support Kubernetes DNS or Consul for production. | 1 day |
| I15 | Cohere error handling | **Add fallback on API error**: If the Cohere reranker returns a non-200 response (not just empty key), fall back to the local reranker. Log the error for monitoring. | 0.5 day |
| I17 | Duplicated init | **Extract a shared initialization library**: Create a `crates/service-init` or similar that provides a `build_search_service(config)` function. Both MCP server and search-worker call this instead of duplicating setup code. | 1 day |
| I18 | Hardcoded chunk sizes | **Make chunking configurable per ingestion request**: Allow the API to accept `chunk_size` and `chunk_overlap` parameters. Store them in `document_jobs` metadata and pass them to the chunker. Provide sensible defaults. | 1 day |

### P3: Future / Nice-to-Have

| # | Improvement | Rationale | Effort |
|---|-------------|-----------|--------|
| 19 | **SPLADE integration** | Replace or augment the local sparse encoder with a learned sparse model (e.g., `naver/splade-v3`). SPLADE provides learned term expansions and domain-specific term weighting. Run inference on GPU for acceptable latency. Pre-compute sparse vectors at indexing time. | 5-10 days |
| 20 | **HyDE (Hypothetical Document Embeddings)** | Before retrieval, have the LLM generate a hypothetical answer, then embed and search with that. The TREC RAG 2025 winner used HyDE Vector Mix and achieved significant gains. | 2-3 days |
| 21 | **Query rewriting / expansion** | Use a small LLM to rewrite user queries before retrieval: expand acronyms, correct typos, decompose compound questions. Glean and Elasticsearch both recommend this. | 2-3 days |
| 22 | **Personalization** | Inject user context (role, team, recent activity) into retrieval. Glean's personalization system is a major differentiator — they consider user role, department, team membership, and interaction history. | 5-10 days |
| 23 | **Permission-aware retrieval** | Add ACL/permission filtering to Qdrant searches. Glean mirrors source-system permissions and filters results accordingly. This is essential for enterprise deployment. | 3-5 days |
| 24 | **Incremental indexing** | Currently, any content change causes a full cascade-delete + reindex of that document's subgraph. Implement incremental updates where possible, especially for AST nodes that haven't changed. | 3-5 days |
| 25 | **Async graph embedding** | Move AST node embedding (Stage 4 of pipeline) to a background task. It doesn't need to block the document's completion status. | 1 day |
| 26 | **Redis-based ingestion queue** | Replace PostgreSQL polling with a Redis queue (BullMQ-like pattern). This provides priority queues, delayed retries, and better visibility into queue depth. | 2-3 days |
| 27 | **Multilingual support** | The current Cohere reranker uses `rerank-english-v3.0` only. Add support for multilingual reranking (`rerank-multilingual-v3.0`) and multilingual embeddings. | 1-2 days |
| 28 | **A/B testing framework** | Allow running two retrieval configurations side-by-side and comparing results. Essential for data-driven optimization of chunking, embedding, and fusion parameters. | 5-10 days |

---

## 9. Comparison with Industry Best Practices

### 9.1 Ingestion Pipeline Comparison

| Aspect | KnowledgeSearch | Pinecone | Glean | Algolia | Elasticsearch | Industry Best |
|--------|----------------|---------|-------|---------|---------------|---------------|
| **Connectors** | Manual API upload + Notion (broken) | SDK-based upsert | 100+ pre-built connectors with permission mirroring | 150+ connectors + Crawler | Beats, Logstash, connectors | Pre-built connectors for major sources |
| **Document parsing** | Docling (Python subprocess), DOCX, TXT | Client-side only | Built-in per-connector | Crawler for web, API for direct | Ingest pipelines with processors | Separate document processing service (Unstructured, Azure Doc Intelligence) |
| **Chunking** | Hierarchical (1500/300), recursive | Client-side (not offered) | Internal (proprietary) | Splits by heading/section | Ingest pipeline processors | Semantic or structure-aware chunking |
| **Embedding** | 3 providers + mock fallback | Integrated `embed` operation | Custom fine-tuned per customer | Client-side only | ML inference node, ELSER | Integrated embedding with managed inference |
| **Change detection** | SHA-256 content hash | Metadata-driven sync | Incremental crawl + webhooks | Partial updates (changed attrs) | _seq_no optimistic concurrency | Incremental + webhook-based near-real-time |
| **Batching** | 50-chunk batches | Batch upsert (max 1000 vectors) | Internal | 10MB or 1000-10000 records | Bulk API | Batched upserts with configurable size |
| **Error handling** | Set status to "failed" | Retry logic in client | Internal retry + DLQ | Timeout + auto-retry | Retry on conflict | Dead-letter queues + retry with backoff |

### 9.2 Search/Retrieval Comparison

| Aspect | KnowledgeSearch | Pinecone | Glean | Algolia | Elasticsearch | Industry Best |
|--------|----------------|---------|-------|---------|---------------|---------------|
| **Dense search** | Yes (3 providers) | Yes (any model) | Yes (custom fine-tuned) | No (keyword + vector beta) | Yes (dense_vector) | Yes |
| **Sparse search** | Hash-based (weak) | Integrated sparse-dense index | BM25 (Lucene) | Proprietary ranking | BM25 + ELSER (SPLADE-like) | BM25 + optional SPLADE |
| **Entity-aware** | Regex + graph traversal | Metadata filter only | Enterprise Graph (comprehensive) | Facets + filters | Nested fields + term queries | Pre-built knowledge graph (Glean) or entity linking |
| **Graph traversal** | 2-hop knowledge graph | Not supported | Enterprise Graph (full) | Not supported | Not natively (can be built) | Graph-enhanced retrieval (Glean, LinkedIn) |
| **Fusion method** | Weighted RRF (4 passes) | Alpha-weighted dot product | Proprietary ensemble | Proprietary | RRF + linear combination | RRF (default) or linear combination |
| **Reranker** | Cohere + Jaccard fallback | Not built-in | Proprietary ML | Custom ranking rules | LR reranker (learned) + script | Cross-encoder reranker (Cohere, BGE-reranker) |
| **Typical p95 latency** | Unknown (no metrics) | <50ms per query | ~100-200ms | <50ms | <100ms | <200ms end-to-end |
| **Query understanding** | None (raw query) | N/A (API-level) | Query rewriting, synonym expansion, acronyms | Typo tolerance, query suggestions | Search analyzers, synonyms, autocomplete | Multi-stage: spelling correction → rewriting → expansion → retrieval |
| **Personalization** | None | Namespace-based | User role, team, history, activity | User token-based | Per-user search | Role, team, history, activity (Glean-scale) |
| **Permission filtering** | Tenant_id only | Namespace isolation | Source-permission mirroring | Secured API keys | Document-level security | Full ACL mirroring from source systems |

### 9.3 Architecture Comparison

| Aspect | KnowledgeSearch | Pinecone Refer. Arch | Glean | Algolia | Elasticsearch | Industry Best |
|--------|----------------|---------|-------|---------|---------------|---------------|
| **Service decomposition** | 5 services + MCP | 3 services (Pelican, Emu, API) | 3 primary components | Unified platform | Single cluster | Microservices with message queue |
| **Queue mechanism** | PostgreSQL polling | SQS | Internal | Push API | Internal | Dedicated message queue (SQS, Kafka, Redis) |
| **Data pipeline** | Sequential within document | SQS + parallel workers | Parallel crawl + index | Push from client | Ingest pipeline | Event-driven pipeline with parallel stages |
| **Session management** | In-memory HashMap | N/A | Database-backed | N/A | N/A | Redis/Valkey or database-backed |
| **Observability** | Structured logging | CloudWatch metrics | Internal | Dashboard | Kibana, Elastic APM | Metrics (p50/p95 latency, error rate, QPS) + structured logging + tracing |
| **MCP support** | Stdio MCP server | N/A | N/A | N/A | N/A | Growing ecosystem |

### 9.4 Key Takeaways from Industry Comparison

1. **KnowledgeSearch's 4-pass hybrid retrieval (dense + sparse + entity + graph) is unusually comprehensive.** Most production systems use 2 passes (dense + BM25). The entity boost and graph traversal are unique differentiators. However, the weak sparse encoder (hash-based instead of BM25/SPLADE) undermines the sparse pass significantly.

2. **The hierarchical chunking strategy is best-in-class.** Industry experts (SuperML, FRENXT, Unstructured) all recommend parent-child chunking. KnowledgeSearch implements this correctly with parent expansion during search. This is the most impactful RAG optimization and it's already done right.

3. **Lack of evaluation is the biggest blind spot.** Every mature search system has systematic evaluation. Glean has ML workflows that run on all indexed content. Pinecone users benchmark with their own data. Without an evaluation framework, all other optimizations are guesswork.

4. **Connector ecosystem is where KnowledgeSearch is weakest.** Glean's 100+ connectors with permission mirroring are its core value proposition. KnowledgeSearch has a broken Notion connector and manual file upload. For enterprise adoption, pre-built connectors to SharePoint, Confluence, Google Drive, Notion, Slack, and email are essential.

5. **The Python subprocess for PDF parsing is a scalability bottleneck.** Industry trend is toward containerized document processing services (Unstructured, Azure Document Intelligence) or Rust-native parsers (pdf-extract, lopdf).

6. **In-memory session storage prevents horizontal scaling.** The industry is moving toward stateless architectures (MCP 2026-07-28 spec removes sessions entirely). KnowledgeSearch's session-per-session memory should be backed by Redis, not a HashMap.

7. **Missing: permission-aware search, personalization, query understanding.** These are table stakes for enterprise search (Glean, Elasticsearch). Without them, KnowledgeSearch is suitable for personal or small-team use but not enterprise deployment.

8. **The self-healing knowledge graph is innovative.** Placeholder node promotion is not something commonly seen in open-source or commercial systems. This is a genuinely interesting design choice that solves the out-of-order ingestion problem cleanly.

---

## 10. Appendix: Key Configuration Values

| Parameter | Default | Location | Notes |
|-----------|---------|----------|-------|
| Chunk size (parent) | 1500 chars | `hierarchical.rs:28` | Hardcoded |
| Chunk size (child) | 300 chars | `hierarchical.rs:29` | Hardcoded |
| Chunk overlap | 200 chars | `recursive.rs:19` | Hardcoded |
| Batch size (embeddings) | 50 | `pipeline.rs` | `process_in_batches` |
| RRF k constant | 60.0 | `fusion.rs` | Standard value |
| Dense weight | 1.0 | `hybrid.rs` | Configurable via API |
| Sparse weight | 1.0 | `hybrid.rs` | Configurable via API |
| Entity weight | 0.8 | `hybrid.rs` | Configurable via API |
| Graph weight | 0.6 | `hybrid.rs` | Configurable via API |
| Poll interval | 2s | `ingestion-worker/main.rs` | Hardcoded |
| Notion sync interval | 30s | `sync-worker/main.rs` | Hardcoded |
| Session message limit | 50 | `agent-core/main.rs` | Hardcoded |
| Search retriever multiplier | 2× limit | `service.rs` | Retrieve 2× then rerank |
| Vector dimension | 1024 | `ingestion-worker/main.rs` | From nv-embedqa-e5-v5 |
| Sparse vocabulary size | 100,000 | `sparse.rs` | Hardcoded |
| Reranker Cohere model | `rerank-english-v3.0` | `rerankers.rs` | Hardcoded |
| Agent LLM | `gemma-4-31b-it` | `agent-core/main.rs` | Gemini model |
| Graph traversal max hops | 2 | `graph_retriever.rs` | Hardcoded |
