import { Elysia, t } from 'elysia';
import { cors } from '@elysiajs/cors';
import postgres from 'postgres';

const port = process.env.PORT || 8000;

const sql = postgres(
  process.env.DATABASE_URL ||
    'postgresql://postgres:postgres@localhost:5432/knowledge_os'
);

const app = new Elysia()
  .use(cors())

  // Global Request Logger
  .onRequest(({ request }) => {
    console.log(
      `[GATEWAY LOG] ${request.method} ${request.url} - ${new Date().toISOString()}`
    );
  })

  // Root Route
  .get('/', () => ({
    message: 'Knowledge-OS API Gateway is online.',
    documentation: '/api/health',
    version: '1.0.0'
  }))

  // Health Check
  .get('/api/health', () => ({
    status: 'healthy',
    uptime: process.uptime(),
    timestamp: new Date().toISOString(),
    services: {
      ingestion_worker: 'online',
      sync_worker: 'online',
      vector_db: 'connected'
    }
  }))

  // Hybrid Search
  .post(
    '/api/search',
    async ({ body }) => {
      const {
        query,
        limit = 5,
        tenant_id = 'default'
      } = body;

      console.log(
        `[SEARCH TRIGGER] Forwarding query to Rust Search Worker: "${query}"`
      );

      const startTime = Date.now();

      try {
        const response = await fetch(
          `http://localhost:8081/search?query=${encodeURIComponent(
            query
          )}&limit=${limit}&tenant_id=${tenant_id}`
        );

        if (!response.ok) {
          throw new Error(
            `Rust Search Worker returned ${response.status}`
          );
        }

        const results = await response.json();

        return {
          query,
          results,
          latency_ms: Date.now() - startTime
        };
      } catch (error) {
        console.error('[SEARCH ERROR]', error);

        return {
          query,
          results: [],
          latency_ms: Date.now() - startTime,
          error: 'Failed to connect to search service'
        };
      }
    },
    {
      body: t.Object({
        query: t.String(),
        limit: t.Optional(t.Numeric()),
        tenant_id: t.Optional(t.String())
      })
    }
  )

  // Generative RAG
  .post(
    '/api/ask',
    async ({ body, set }) => {
      const { question } = body;
      console.log(`[RAG TRIGGER] Forwarding to Agent Service: "${question}"`);

      try {
        const response = await fetch('http://localhost:8001/ask', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ query: question })
        });

        if (!response.ok) {
            set.status = response.status;
            return { error: 'Agent failed' };
        }

        // Forward the stream directly
        return new Response(response.body, {
          headers: { 'Content-Type': 'text/plain' }
        });
      } catch (error) {
        console.error('[RAG ERROR]', error);
        return {
          answer: 'Failed to connect to the agent.',
          error: 'Service unavailable'
        };
      }
    },
    {
      body: t.Object({
        question: t.String(),
        tenant_id: t.Optional(t.String())
      })
    }
  )

  // Document Ingestion
  .post(
    '/api/documents',
    async ({ body }) => {
      const {
        title,
        content,
        fileExtension
      } = body;

      const tenant_id = 'default';

      console.log(
        `[INGEST TRIGGER] Saving document job: "${title}"`
      );

      try {
        const result = await sql`
          INSERT INTO document_jobs (
            id,
            tenant_id,
            title,
            content,
            status,
            file_extension
          )
          VALUES (
            ${crypto.randomUUID()},
            ${tenant_id},
            ${title},
            ${content},
            'pending',
            ${fileExtension ?? null}
          )
          RETURNING id
        `;

        return {
          success: true,
          document_id: result[0].id,
          status: 'queued',
          message:
            'Ingestion pipeline successfully triggered. The background Rust Ingestion Worker will process the task.'
        };
      } catch (error) {
        console.error('[INGEST ERROR]', error);

        return {
          success: false,
          error: 'Failed to queue document ingestion'
        };
      }
    },
    {
      body: t.Object({
        title: t.String(),
        content: t.String(),
        fileExtension: t.Optional(t.String())
      })
    }
  )

  // Connector List
  .get('/api/connectors', () => ({
    connectors: [
      {
        id: 'conn_notion_001',
        type: 'Notion',
        name: 'Company Knowledge Base',
        status: 'connected',
        last_sync: '2026-05-29T11:47:00Z',
        tenant_id: 'default'
      },
      {
        id: 'conn_slack_002',
        type: 'Slack',
        name: '#eng-announcements',
        status: 'disconnected',
        last_sync: null,
        tenant_id: 'default'
      }

    ]
  }))

  // Trigger Connector Sync
  .post('/api/connectors/:id/sync', ({ params }) => {
    console.log(
      `[SYNC TRIGGER] Triggering sync connector task for: "${params.id}"`
    );

    return {
      success: true,
      connector_id: params.id,
      status: 'running',
      message:
        'Sync crawler task dispatched to background Rust Sync Worker.'
    };
  })

  // Get Document Ingestion Status
  .get('/api/documents/:id/status', async ({ params }) => {
    const result = await sql`
      SELECT progress_stage, progress_percent, progress_message, status 
      FROM document_jobs 
      WHERE id = ${params.id}
    `;
    
    if (result.length === 0) return { error: "Job not found" };
    
    return {
      stage: result[0].progress_stage,
      percent: result[0].progress_percent,
      message: result[0].progress_message,
      status: result[0].status
    };
  })

  .listen(port);

console.log(
  `[GATEWAY STARTED] Bun API Gateway is running at http://localhost:${port}`
);