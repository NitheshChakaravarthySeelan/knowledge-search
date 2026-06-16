# KnowledgeSearch

A production-grade hybrid RAG engine with entity-boosted retrieval, session memory, MCP server, and self-improving knowledge graphs — built in Rust + TypeScript.

```text
     User (Web / MCP / API)
           |
           v
    API Gateway (Bun/Elysia)
           |
     +-----+-----+
     |           |
  search       ask
     |           |
     v           v
 search-worker  agent-core
     |           |
     v           v
    Qdrant <---> LLM (Gemini/OpenAI/NVIDIA)
     |
     v
 ingestion-worker (parsers → chunkers → entity extract → embed → upsert)
     |
     v
 PostgreSQL (kb_nodes, kb_graph_edges, document_jobs)
```

## Features

### Search
- **Hybrid dense + sparse + entity-boosted retrieval** — three independent passes fused via weighted Reciprocal Rank Fusion (RRF)
- **Entity extraction at search time** — regex-based heuristics extract capitalized phrases, CamelCase terms, acronyms, and quoted terms from queries for an additional entity-aware search pass
- **Cross-encoder reranking** — Cohere rerank (with fallback to local bigram Jaccard similarity when no API key is set)
- **Configurable weights** — `dense_weight`, `sparse_weight`, `entity_weight` per query

### Ingestion
- **Multi-format parsing** — PDF (via Docling), DOCX, Markdown, plain text
- **AST-level graph extraction** — parses Rust, Python, JS/TS files to extract classes, functions, imports, and relationships (IMPORTS, DEFINES, REFERENCES, CALLS, IMPLEMENTS)
- **Entity annotation** — every chunk is annotated with extracted entities, entity names, and a RFC3339 `ingested_at` timestamp
- **Hierarchical chunking** — parent (1500 char) / child (300 char) chunks with parent-context expansion during search
- **Self-healing graph** — when an edge references a not-yet-ingested node, a `Placeholder` node is created so all back-links survive
- **SHA-256 change detection** — skips re-indexing when content hasn't changed

### Memory
- **Per-session conversation memory** — in-memory `HashMap<session_id, VecDeque<(Q, A)>>` storing last 10 turns
- **History injection** — past Q&A pairs are injected as conversation preamble before each query
- **SSE streaming ask endpoint** — real-time token-by-token responses with reasoning traces

### MCP Server (Model Context Protocol)
- **`search_knowledge_base`** tool — hybrid search over your knowledge base
- **`ingest_pdf`** tool — ingest a PDF and return its markdown representation
- Stdio transport via `rmcp` v1.7.0 — works with Claude Code, Cursor, VS Code Copilot, and any MCP-compatible agent

### Embedding Providers
| Provider | Model | Dimensions |
|----------|-------|-----------|
| OpenAI | `text-embedding-3-small` | 1536 |
| Gemini | `text-embedding-004` | 768 |
| NVIDIA | `nv-embedqa-e5-v5` | 1024 |

All providers have a deterministic sandbox fallback — no API keys needed for development. Dense + sparse embeddings stored in Qdrant with `dense-text` and `sparse-text` named vector fields.

### LLM Providers
| Provider | Model |
|----------|-------|
| OpenAI | `gpt-4o-mini` |
| Gemini | `gemini-1.5-flash` |
| NVIDIA | `meta/llama-3.3-70b-instruct` |

### Frontend (Next.js)
- Obsidian dark-theme glassmorphism UI built with vanilla CSS
- Real-time streaming Q&A interface with session support
- Document management, ingestion status, and search explorer

### Sandbox Mode
Everything works offline with zero API keys — embeddings fall back to deterministic hash vectors, LLM returns templated responses, Notion connector returns seeded sample data, reranker falls back to local Jaccard similarity.

## Quick Start

```bash
# 1. Start infrastructure
docker compose up -d

# 2. Run migrations
cargo run -p migration

# 3. API Gateway
cd apps/api && bun install && bun run dev

# 4. Frontend
cd apps/web && bun install && bun run dev

# 5. Ingestion worker
cargo run -p ingestion-worker

# 6. Search worker
cargo run -p search-worker

# 7. Agent core (RAG streaming)
cargo run -p agent-core

# 8. MCP server (for agent tools)
cargo run -p mcp-server
```

## Architecture

```
apps/
  api/          Bun + Elysia API gateway (:8000)
  web/          Next.js frontend (:3000)

crates/
  common/       Shared config, errors, telemetry
  entities/     SeaORM entity models
  migration/    Database migrations
  documents/    Loaders, parsers, chunkers, entity/graph extraction
  embeddings/   Dense + sparse embedding providers
  search/       Hybrid retriever, RRF fusion, rerankers
  llm/          LLM providers + RAG service
  connectors/   Qdrant client, Notion connector
  mcp-server/   MCP stdio server with search + ingest tools
  agent-core/   Axum SSE streaming agent with session memory

services/
  ingestion-worker/  Document ingestion pipeline (poll → parse → chunk → embed → upsert)
  search-worker/     Search HTTP API (:8081)
  sync-worker/       Notion sync cron
```

## Tech Stack

- **Rust** — core libraries, workers, MCP server, agent core
- **TypeScript / Bun** — API gateway, frontend
- **Qdrant** — vector database (dense + sparse named vectors)
- **PostgreSQL 16** — document jobs, knowledge graph (nodes + edges)
- **Next.js** — web UI with glassmorphism dark theme

## License

MIT
