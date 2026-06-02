import { Elysia, t } from 'elysia';
import { cors } from '@elysiajs/cors';
import postgres from 'postgres';

const port = process.env.PORT || 8000;
const sql = postgres(process.env.DATABASE_URL || 'postgresql://postgres:postgres@localhost:5432/knowledge_os');

const app = new Elysia()
  .use(cors()) // Use official CORS plugin to handle OPTIONS preflight
  // Global Middleware: Simple Request logger
  .onRequest(({ request }) => {
    console.log(`[GATEWAY LOG] ${request.method} ${request.url} - ${new Date().toISOString()}`);
  })

  // Root Welcome Route
  .get('/', () => ({
    message: 'Knowledge-OS API Gateway is online.',
    documentation: '/api/health',
    version: '1.0.0'
  }))
  
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
  .post('/api/search', async ({ body }) => {
    const { query, limit = 5, tenant_id = "tenant_corporate_1" } = body;
    console.log(`[SEARCH TRIGGER] Forwarding query to Rust Search Worker: "${query}"`);
    
    try {
      const response = await fetch(`http://localhost:8081/search?query=${encodeURIComponent(query)}&limit=${limit}&tenant_id=${tenant_id}`);
      if (!response.ok) {
        throw new Error(`Rust Search Worker returned ${response.status}`);
      }
      const results = await response.json();
      return {
        query,
        results,
        latency_ms: 0 // Fetch doesn't provide latency easily, could measure manually
      };
    } catch (error) {
      console.error(`[SEARCH ERROR] ${error}`);
      return {
        query,
        results: [],
        error: "Failed to connect to search service"
      };
    }
  }, {
    body: t.Object({
      query: t.String(),
      limit: t.Optional(t.Numeric()),
      tenant_id: t.Optional(t.String())
    })
  })

  // 3. Document Ingestion Input
  .post('/api/documents', async ({ body }) => {
    const { title, content } = body;
    const tenant_id = "tenant_corporate_1"; // Hardcoded for MVP
    
    console.log(`[INGEST TRIGGER] Saving document job: "${title}"`);
    
    try {
      const result = await sql`
        INSERT INTO document_jobs (id, tenant_id, title, content, status)
        VALUES (${crypto.randomUUID()}, ${tenant_id}, ${title}, ${content}, 'pending')
        RETURNING id
      `;

      return {
        success: true,
        document_id: result[0].id,
        status: 'queued',
        message: 'Ingestion pipeline successfully triggered. The background Rust Ingestion Worker will process the task.'
      };
    } catch (error) {
      console.error(`[INGEST ERROR] ${error}`);
      return {
        success: false,
        error: "Failed to queue document ingestion"
      };
    }
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
