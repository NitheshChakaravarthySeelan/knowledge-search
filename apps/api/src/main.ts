useElysia(); // Setup Elysia
import { Elysia, t } from 'elysia';

const port = process.env.PORT || 8000;

const app = new Elysia()
  // Global Middleware: Simple Request logger
  .onRequest(({ request }) => {
    console.log(`[GATEWAY LOG] ${request.method} ${request.url} - ${new Date().toISOString()}`);
  })
  
  // CORS configuration response headers
  .onBeforeResponse(({ set }) => {
    set.headers['Access-Control-Allow-Origin'] = '*';
    set.headers['Access-Control-Allow-Methods'] = 'GET, POST, PUT, DELETE, OPTIONS';
    set.headers['Access-Control-Allow-Headers'] = 'Content-Type, Authorization';
  })

  // 1. Health Probe
  .get('/api/health', () => {
    return {
      status: 'healthy',
      uptime: process.uptime(),
      timestamp: new Date().toISOString(),
      services: {
        ingestion_worker: 'online',
        sync_worker: 'online',
        vector_db: 'connected'
      }
    };
  })

  // 2. Hybrid Reranked Vector Search
  .post('/api/search', ({ body }) => {
    const { query, limit = 5 } = body;
    console.log(`[SEARCH TRIGGER] Executing hybrid query: "${query}" (limit: ${limit})`);
    
    // Simulate routing search request to Rust search-worker / service
    return {
      query,
      results: [
        {
          chunk_id: "notion_page_101_chunk_0",
          document_id: "notion_page_101",
          content: "Welcome to Knowledge-OS! This document outlines our high-performance Rust monorepo context. We use Cargo Workspace and Bun.",
          score: 0.87,
          metadata: {
            source: "Notion",
            title: "Engineering Onboarding Roadmap",
            url: "https://notion.so/knowledge-os/Engineering-Onboarding-Roadmap"
          }
        },
        {
          chunk_id: "notion_page_102_chunk_0",
          document_id: "notion_page_102",
          content: "Our system operates on Postgres for traditional metadata tracking and transactional entities, and Qdrant for storing and querying text vector embeddings.",
          score: 0.76,
          metadata: {
            source: "Notion",
            title: "Database Strategy Draft",
            url: "https://notion.so/knowledge-os/Database-Strategy-Draft"
          }
        }
      ],
      latency_ms: 12
    };
  }, {
    body: t.Object({
      query: t.String(),
      limit: t.Optional(t.Numeric())
    })
  })

  // 3. Document Ingestion Input
  .post('/api/documents', ({ body }) => {
    const { title, content } = body;
    console.log(`[INGEST TRIGGER] Dispatching document to Rust Ingestion Worker: "${title}"`);
    
    return {
      success: true,
      document_id: `doc_${Math.random().toString(36).substring(7)}`,
      status: 'queued',
      message: 'Ingestion pipeline successfully triggered. The background Rust Ingestion Worker is processing the task.'
    };
  }, {
    body: t.Object({
      title: t.String(),
      content: t.String()
    })
  })

  // 4. Connector Accounts & Schedules
  .get('/api/connectors', () => {
    return {
      connectors: [
        {
          id: "conn_notion_001",
          type: "Notion",
          name: "Company Knowledge Base",
          status: "connected",
          last_sync: "2026-05-29T11:47:00Z"
        },
        {
          id: "conn_slack_002",
          type: "Slack",
          name: "#eng-announcements",
          status: "disconnected",
          last_sync: null
        }
      ]
    };
  })

  // 5. Trigger Immediate Crawler Sync
  .post('/api/connectors/:id/sync', ({ params }) => {
    console.log(`[SYNC TRIGGER] Triggering sync connector task for: "${params.id}"`);
    return {
      success: true,
      connector_id: params.id,
      status: 'running',
      message: 'Sync crawler task dispatched to background Rust Sync Worker.'
    };
  })

  .listen(port);

console.log(`[GATEWAY STARTED] Bun API Gateway is running at http://localhost:${port}`);

function useElysia() {}
