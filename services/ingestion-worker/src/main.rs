use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tracing::{info, warn, error};
use sea_orm::{Database, DatabaseConnection, EntityTrait, QueryFilter, ColumnTrait, ConnectionTrait, Statement};

mod pipeline;

use common::config::AppConfig;
use common::telemetry::init_telemetry;
use common::types::{DocumentId, TenantId};
use migration::Migrator;
use entities::document_job;
use sea_orm_migration::MigratorTrait;

use connectors::QdrantClient;
use documents::ParserRegistry;
use embeddings::{EmbeddingProvider, SparseEmbeddingProvider, OpenAiProvider, GeminiProvider, NvidiaProvider, BM25SparseEncoder};

// ─── Concurrency Configuration ───────────────────────────────────────────

/// Default maximum number of documents to process concurrently.
/// We use `tokio::sync::Semaphore` to bound concurrency rather than spawning
/// unlimited tasks, because:
///   1. Embedding APIs (NVIDIA, OpenAI, Gemini) have rate limits
///   2. PDF parsing spawns Python subprocesses (memory-heavy)
///   3. Each concurrent job holds a DB connection from the pool
///   4. Too many concurrent Qdrant writes can degrade query performance
///
/// The sweet spot depends on your embedding provider's rate limits.
/// NVIDIA's nv-embedqa-e5-v5 typically allows ~100 RPM; with batch_size=50
/// and each doc producing ~10-20 batches, 4 concurrent docs = ~40-80 requests
/// per minute, well within limits. Adjust via MAX_INGESTION_CONCURRENCY env var.
const DEFAULT_MAX_CONCURRENCY: usize = 4;

// ─── Entry Point ─────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    init_telemetry("ingestion-worker");
    info!("Starting Ingestion Worker daemon...");

    // Load configuration from environment / .env
    let config = match AppConfig::load_from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load configuration: {:?}", e);
            std::process::exit(1);
        }
    };

    // Connect to PostgreSQL and run pending migrations
    let db: DatabaseConnection = match Database::connect(&config.database_url).await {
        Ok(conn) => {
            info!("Connected to PostgreSQL database.");
            conn
        }
        Err(e) => {
            error!("Failed to connect to database: {:?}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = Migrator::up(&db, None).await {
        error!("Failed to run database migrations: {:?}", e);
        std::process::exit(1);
    }
    info!("Database migrations completed successfully.");

    // Select embedding provider with fallback chain
    let embedding_provider: Arc<dyn EmbeddingProvider> = if let Some(key) = &config.nvidia_api_key {
        info!("NVIDIA API key found, selecting NVIDIA embedding provider.");
        Arc::new(NvidiaProvider::new(key.clone()))
    } else if let Some(key) = &config.gemini_api_key {
        info!("Gemini API key found, selecting Gemini embedding provider.");
        Arc::new(GeminiProvider::new(key.clone()))
    } else if let Some(key) = &config.openai_api_key {
        info!("OpenAI API key found, selecting OpenAI embedding provider.");
        Arc::new(OpenAiProvider::new(key.clone()))
    } else {
        warn!("No API keys found. Falling back to local sandbox mock embeddings.");
        Arc::new(NvidiaProvider::new("mock".to_string()))
    };

    // Initialize shared pipeline components
    let parser_registry = Arc::new(ParserRegistry::new());

    let bm25_stats_path = std::env::var("BM25_STATS_PATH")
        .unwrap_or_else(|_| "./data/bm25_stats.json".to_string());
    let sparse_encoder = Arc::new(BM25SparseEncoder::with_persistence(bm25_stats_path));

    let qdrant_client = match QdrantClient::new(&config.qdrant_url) {
        Ok(client) => {
            info!(url = config.qdrant_url, "Connected to Qdrant vector database.");
            Arc::new(client)
        }
        Err(e) => {
            error!("Failed to connect to Qdrant: {:?}", e);
            std::process::exit(1);
        }
    };

    let collection_name = "knowledge_base";
    let vector_dim = embedding_provider.dimension() as u64;
    if let Err(e) = qdrant_client.ensure_collection(collection_name, vector_dim).await {
        error!("Failed to ensure Qdrant collection: {:?}", e);
    }

    let ingestion_pipeline = Arc::new(pipeline::IngestionPipeline::new(
        db.clone(),
        qdrant_client.clone(),
        embedding_provider.clone(),
        sparse_encoder.clone(),
    ));

    // ═══════════════════════════════════════════════════════════════════════
    //  MAIN POLL LOOP with bounded concurrent processing
    // ═══════════════════════════════════════════════════════════════════════
    //
    // Design rationale:
    //
    //  Why spawn tasks instead of processing sequentially?
    //  ──────────────────────────────────────────────
    //  Sequential processing means one slow document (e.g., a 200-page PDF
    //  that takes 60s to parse + embed) blocks all other pending jobs.
    //  With concurrent tasks, independent jobs make progress simultaneously,
    //  dramatically reducing total wall-clock time for bulk uploads.
    //
    //  Why use a Semaphore (not unbounded tokio::spawn)?
    //  ──────────────────────────────────────────────
    //  Unbounded spawning would let the number of concurrent jobs grow
    //  without limit. Each job holds DB connections, makes embedding API
    //  calls, and may spawn Python subprocesses. A semaphore provides
    //  backpressure: when the limit is reached, new tasks wait at the
    //  acquire() call before starting, naturally throttling intake.
    //
    //  Why wait for all tasks between poll cycles?
    //  ──────────────────────────────────────────
    //  If we spawned tasks in the background and polled immediately again,
    //  we'd accumulate unbounded pending tasks. By awaiting all handles
    //  before the next poll, we create a natural batch boundary: the worker
    //  processes one wave of pending jobs at a time. No task accumulation.
    //
    //  Why the atomic UPDATE to claim jobs?
    //  ──────────────────────────────────
    //  The poll loop selects all jobs WHERE status = 'pending'. Without an
    //  atomic claim, a subsequent poll could see the same jobs before their
    //  spawned tasks update the status. By doing an UPDATE ... WHERE status
    //  = 'pending' and checking rows_affected, we ensure each job is claimed
    //  exactly once per poll cycle. If another poll cycle overlaps (edge case
    //  with very long docs), the second attempt finds status != 'pending'
    //  and skips.
    //
    //  Why default concurrency of 4?
    //  ──────────────────────────
    //  - Embedding API rate limits: most providers allow 50-100 requests/min.
    //    With batch_size=50 and ~10 batches/doc, 4 concurrent docs = ~40
    //    requests/minute — safe headroom.
    //  - Memory: PDF parsing via Python subprocess uses ~100-200MB per doc.
    //    4 concurrent = ~800MB peak, reasonable for most deployments.
    //  - DB connections: SeaORM pool default is ~10-20 connections.
    //    4 concurrent jobs + worker overhead = ~5-6 simultaneous connections.
    // ═══════════════════════════════════════════════════════════════════════

    let max_concurrency: usize = std::env::var("MAX_INGESTION_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_CONCURRENCY);

    info!(
        max_concurrency = max_concurrency,
        "Ingestion Worker listening for pending document jobs."
    );

    let semaphore = Arc::new(Semaphore::new(max_concurrency));

    loop {
        // ── Fetch pending jobs ──────────────────────────────────────────
        let pending_jobs = match document_job::Entity::find()
            .filter(document_job::Column::Status.eq("pending"))
            .all(&db)
            .await
        {
            Ok(jobs) => jobs,
            Err(e) => {
                error!("Failed to fetch pending jobs: {:?}", e);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        if pending_jobs.is_empty() {
            sleep(Duration::from_secs(2)).await;
            continue;
        }

        info!(
            count = pending_jobs.len(),
            "Fetched pending document jobs."
        );

        // ── Spawn a concurrent task per job ─────────────────────────────
        // Each task: (1) acquires a semaphore permit (may wait), (2) claims
        // the job atomically, (3) runs the pipeline, (4) updates status.
        let mut handles = Vec::with_capacity(pending_jobs.len());

        for job in pending_jobs {
            let sem_clone = Arc::clone(&semaphore);
            let db_clone = db.clone();
            let parser_registry_clone = Arc::clone(&parser_registry);
            let pipeline_clone = Arc::clone(&ingestion_pipeline);
            let collection_name = collection_name.to_string();

            // Why spawn a task per job:
            // We want true concurrency — not just async interleaving on a
            // single thread. Each job involves CPU-heavy parsing (Python
            // subprocess) and blocking I/O (embedding API calls). tokio::spawn
            // distributes work across the thread pool, keeping the poll loop
            // responsive and maximizing CPU utilization.
            let handle = tokio::spawn(async move {
                // Acquire a semaphore permit BEFORE processing.
                // If all slots are full, this await blocks until a running
                // job finishes and releases its permit. This provides
                // automatic backpressure without any queue management.
                let _permit = sem_clone.acquire_owned().await
                    .expect("Semaphore closed unexpectedly");
                process_single_job(
                    job,
                    db_clone,
                    parser_registry_clone,
                    pipeline_clone,
                    &collection_name,
                ).await;
                // `_permit` is dropped here → semaphore releases a slot
            });

            handles.push(handle);
        }

        // ── Wait for all tasks in this batch ────────────────────────────
        // We wait for ALL tasks to finish before polling again. This
        // prevents task accumulation across poll cycles. Even though tasks
        // run concurrently (up to `max_concurrency` at a time), all must
        // complete before we check for new jobs.
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Task panicked: {:?}", e);
            }
        }

        // ── Persist BM25 statistics after each batch ────────────────────
        // After processing a wave of documents, flush IDF statistics to
        // disk so they survive a crash. This is a no-op for non-BM25
        // sparse providers.
        if let Err(e) = sparse_encoder.persist_stats().await {
            warn!(error = %e, "Failed to persist BM25 statistics");
        }

        // No sleep here because we just finished processing all pending
        // jobs. The loop will immediately check for new ones. If none
        // exist, the empty-branch sleep(2s) kicks in.
    }
}

// ─── Single Job Processor ────────────────────────────────────────────────

/// Process a single ingestion job: parse content, run pipeline, update status.
///
/// Extracted as a standalone function (instead of inline closures) for:
///   1. **Readability** — separates "orchestration logic" (main loop, semaphore,
///      claiming) from "processing logic" (parsing, pipeline execution).
///   2. **Ownership clarity** — each argument is either `Clone` or consumed;
///      no complex borrows across IIFE boundaries.
///   3. **Testability** — can be tested in isolation with mock jobs.
///
/// Design: We separate status claiming from processing because the atomic
/// UPDATE (status = 'processing') must happen inside the spawned task to
/// correctly handle the race between concurrent tasks within the same poll
/// cycle. If the claim fails (rows_affected == 0), we return early.
async fn process_single_job(
    job: document_job::Model,
    db: DatabaseConnection,
    parser_registry: Arc<ParserRegistry>,
    pipeline: Arc<pipeline::IngestionPipeline>,
    collection_name: &str,
) {
    let tenant = TenantId(job.tenant_id.clone());
    let doc_id = DocumentId(job.id.to_string());
    let job_id = job.id;

    info!(
        tenant = tenant.0,
        document = doc_id.0,
        title = job.title,
        "Starting job processing (may wait for semaphore)."
    );

    // ── Atomic Claim ─────────────────────────────────────────────────────
    // Atomically transition status from 'pending' → 'processing'.
    // The WHERE status = 'pending' ensures only one task claims this job,
    // even if multiple tasks see the same job row (e.g., due to overlapped
    // poll cycles or multiple worker instances).
    //
    // We use a raw SQL UPDATE rather than SeaORM's ActiveModel::update()
    // because the latter only filters by primary key — it can't conditionally
    // check the current status. Raw SQL gives us the atomic conditional
    // update we need.
    //
    // Statement::from_sql_and_values properly parameterizes the query,
    // preventing SQL injection. The `$3` return only returns something if
    // the row was actually updated (one row affected).
    let claim_sql = Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        r#"UPDATE document_jobs SET status = 'processing' WHERE id = $1 AND status = 'pending'"#,
        [job_id.into()],
    );

    match db.execute(claim_sql).await {
        Ok(result) => {
            // SeaORM's ExecResult.rows_affected() tells us how many rows
            // the UPDATE actually modified. If 0, the job was already
            // claimed by another task/worker — we skip it silently.
            if result.rows_affected() == 0 {
                info!(
                    job_id = job.id.to_string(),
                    "Job already claimed by another worker, skipping."
                );
                return;
            }
        }
        Err(e) => {
            error!(
                job_id = job.id.to_string(),
                error = %e,
                "Failed to claim job (status → processing)"
            );
            return;
        }
    }

    // ── Parse & Pipeline ─────────────────────────────────────────────────
    let pipeline_result = run_pipeline(
        &job,
        &parser_registry,
        &pipeline,
        &tenant,
        collection_name,
    )
    .await;

    // ── Update Final Status ──────────────────────────────────────────────
    let final_status = match &pipeline_result {
        Ok(_) => {
            info!(job_id = job.id.to_string(), "Job completed successfully.");
            "completed"
        }
        Err(e) => {
            error!(job_id = job.id.to_string(), "Job failed: {}", e);
            "failed"
        }
    };

    // Update the job's final status. If this fails, we log and move on —
    // the job row still shows 'processing', which acts as a dead-letter
    // indicator for manual inspection.
    let stmt = Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        "UPDATE document_jobs SET status = $1 WHERE id = $2",
        [final_status.into(), job_id.into()],
    );

    if let Err(e) = db.execute(stmt).await {
        error!(
            job_id = job.id.to_string(),
            status = final_status,
            error = %e,
            "Failed to update final job status"
        );
    }
}

/// Runs the actual ingestion pipeline (parsing → pipeline stages).
///
/// Separated from `process_single_job` to keep the "claim → process →
/// finalize" flow readable. This function returns `Result<(), String>`
/// so the caller can decide what status to set.
async fn run_pipeline(
    job: &document_job::Model,
    parser_registry: &ParserRegistry,
    pipeline: &pipeline::IngestionPipeline,
    tenant: &TenantId,
    collection_name: &str,
) -> Result<(), String> {
    // ── Decode ──────────────────────────────────────────────────────────
    // If the file is binary (PDF/DOCX), the API gateway stored it as
    // base64 in the `content` column. We decode it back to bytes before
    // passing to the parser.
    let raw_bytes = if job.file_extension.as_deref() == Some("pdf")
        || job.file_extension.as_deref() == Some("docx")
    {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(&job.content)
            .map_err(|e| format!("Base64 decode failed: {:?}", e))?
    } else {
        job.content.as_bytes().to_vec()
    };

    // ── Parse ───────────────────────────────────────────────────────────
    // Route to the appropriate parser based on file extension. If no
    // parser is registered for the extension, fall back to treating the
    // content as raw UTF-8 text (works for .txt, .md, code files, etc.).
    let extracted_content = if let Some(ext) = &job.file_extension {
        match parser_registry.get_parser(ext) {
            Some(parser) => parser
                .parse(&raw_bytes)
                .map_err(|e| format!("Parser failed: {:?}", e))?,
            None => {
                warn!(extension = ext, "No specialized parser found, trying plain text fallback.");
                String::from_utf8(raw_bytes)
                    .map_err(|e| format!("UTF8 conversion failed: {:?}", e))?
            }
        }
    } else {
        String::from_utf8(raw_bytes)
            .map_err(|e| format!("UTF8 conversion failed: {:?}", e))?
    };

    // ── Resolve identifiers ────────────────────────────────────────────
    let file_path = job.file_path.as_deref().unwrap_or(&job.title);

    let source_type_str = match job.file_extension.as_deref() {
        Some("rs") | Some("py") | Some("js") | Some("ts") | Some("go")
        | Some("java") | Some("cpp") | Some("c") => "GitHub",
        _ => "FileUpload",
    };

    // ── Pipeline ───────────────────────────────────────────────────────
    pipeline
        .process_job(
            job,
            tenant,
            source_type_str,
            file_path,
            &job.title,
            &extracted_content,
            job.file_extension.as_deref(),
            collection_name,
        )
        .await
        .map_err(|e| format!("Pipeline failed: {:?}", e))?;

    Ok(())
}
