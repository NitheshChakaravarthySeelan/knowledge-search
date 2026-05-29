# Knowledge-OS | High-Performance AI Knowledge Infrastructure Monorepo

Knowledge-OS is an enterprise-grade, high-performance AI knowledge indexing and hybrid search platform designed around **bounded contexts** and **microservices**. Built with **Rust** for heavy compute, extraction, chunking, and search ranking, and **TypeScript + Bun** for the web frontend and API gateway orchestration, it mirrors the resilient architectures of search databases like Qdrant and Quickwit.

---

## 🏛 Architecture Overview

```text
                                 ┌────────────────────────┐
                                 │      Web Browser       │
                                 │    (Next.js Client)    │
                                 └───────────┬────────────┘
                                             │ HTTP
                                             ▼
                                 ┌────────────────────────┐
                                 │    Bun API Gateway     │
                                 │     (apps/api)         │
                                 └───────────┬────────────┘
                                             │
                       ┌─────────────────────┴─────────────────────┐
                       ▼ Job Dispatch (DB / Queue)                 ▼ HTTP/gRPC Queries
             ┌───────────────────┐                       ┌───────────────────┐
             │    Sync Worker    │                       │   Search Worker   │
             │   (sync-worker)   │                       │  (search-worker)  │
             └─────────┬─────────┘                       └─────────┬─────────┘
                       │ Notion Pages                              │ Embed & Query
                       ▼                                           ▼
             ┌───────────────────┐                       ┌───────────────────┐
             │ Ingestion Worker  │                       │ Vector Database   │
             │(ingestion-worker) │                       │     (Qdrant)      │
             └───────────────────┘                       └───────────────────┘
```

The system separates concerns into:
1. **API Gateway (`apps/api`)**: Built on Bun and Elysia for blazing-fast TypeScript request routing and session handling.
2. **Web Frontend (`apps/web`)**: Next.js & React SPA featuring a stunning, responsive **Glassmorphism Obsidian dark-mode custom styling system** written entirely in Vanilla CSS.
3. **Core Libraries (`crates/`)**: Modular, domain-focused Rust packages handling data modeling, telemetry, documents processing, vector embedding, and hybrid search.
4. **Daemons / Workers (`services/`)**: High-performance Rust binary services executing background crawler synchronization (`sync-worker`) and the document indexing pipeline (`ingestion-worker`).

---

## 📂 Directory Layout

```text
knowledge-os/
├── apps/                        # Entrypoints
│   ├── api/                     # TS API Gateway (Bun & Elysia) - [Day 1 Core]
│   ├── web/                     # TS Next.js UI Dashboard - [Day 1 Core]
│   ├── admin/                   # TS Admin dashboard (Future Growth)
│   └── cli/                     # Rust CLI tools (Future Growth)
├── crates/                      # Core Business Logic (Rust Workspace Libraries)
│   ├── common/                  # Configuration, logging, unified errors - [Day 1 Core]
│   ├── documents/               # Loaders, chunkers, parsers - [Day 1 Core]
│   ├── embeddings/              # Vector providers (Gemini, OpenAI) - [Day 1 Core]
│   ├── search/                  # Hybrid retrievers & rerankers - [Day 1 Core]
│   ├── llm/                     # LLM clients & prompt templaters - [Day 1 Core]
│   ├── connectors/              # Integrations (Notion crawler) - [Day 1 Core]
│   └── (auth, ingestion, entities, permissions, analytics, events) # (Future growth)
├── services/                    # Long-Running Workers (Rust Binaries)
│   ├── ingestion-worker/        # Document processor queue consumer - [Day 1 Core]
│   ├── sync-worker/             # Connection poller cron service - [Day 1 Core]
│   └── (embedding-worker, search-worker, analytics-worker, scheduler) # (Future growth)
├── deploy/                      # Infrastructure & monitoring configurations (Docker, Kubernetes)
├── scripts/                     # Operations and data seeding commands
├── tests/                       # Global monorepo integration suites
└── Cargo.toml                   # Root Cargo workspace manifest
```

---

## ⚡️ Zero-Config Sandbox Mode

To allow instant testing and developer onboarding, all microservices and providers feature a **High-Fidelity Sandbox Fallback Mode**:
- **Embeddings / LLM**: If OpenAI or Gemini keys are missing, the system generates deterministic character-bi-gram-based semantic hash vectors locally. You can execute full searches and LLM text generation entirely offline without setting up external bills.
- **Connectors**: If Notion developer tokens are missing, the Notion Client injects pre-seeded realistic company documents (Engineering Roadmaps, Database Strategies) to simulate realistic ingestion crawls.

---

## 🚀 Getting Started

### 1. Requirements
Ensure you have the following installed on your machine:
- **Rust** (stable, Cargo 1.75+)
- **Bun** (for TypeScript execution, or `npm` / `node` as alternative fallback runner)

---

### 2. Running the TypeScript API Gateway (Bun)
Navigate to the gateway folder and start the developer server:
```bash
cd apps/api
bun install
bun run dev
```
The gateway will bootstrap on `http://localhost:8000`.

---

### 3. Running the Next.js Frontend (Bun)
In another terminal, start the Next.js web interface:
```bash
cd apps/web
bun install
bun run dev
```
The Obsidian dashboard will open on `http://localhost:3000`.

---

### 4. Running the Rust Background Daemons
From the monorepo root, execute the background daemons in separate terminal tabs:

**A. Ingestion Worker Pipeline:**
```bash
cargo run -p ingestion-worker
```

**B. Notion Connection Sync Crawler:**
```bash
cargo run -p sync-worker
```

---

### 5. Running Cargo Tests and Checks
Verify that the entire monorepo compile-checks cleanly and test suites execute flawlessly:
```bash
# Verify compiler builds
cargo check

# Run unit tests across all libraries
cargo test
```

---

## 🛡 System Telemetry
The Rust services initialize the observability stack using `tracing` + `tracing-subscriber`. Log levels can be adjusted on boot using the standard `RUST_LOG` environment variable:
```bash
RUST_LOG=debug cargo run -p ingestion-worker
```
This prints precise, structured operational state traces for the extraction, chunking, and embedding stages.
