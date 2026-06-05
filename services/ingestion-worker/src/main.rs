use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error};
use sea_orm::{Database, DatabaseConnection, EntityTrait, QueryFilter, ColumnTrait, ActiveModelTrait, ActiveValue};

use common::config::AppConfig;
use common::telemetry::init_telemetry;
use common::types::{DocumentId, TenantId};
use migration::Migrator;
use entities::document_job;
use sea_orm_migration::MigratorTrait;

use connectors::QdrantClient;
use documents::loaders::{Loader, FileLoader};
use documents::chunkers::{Chunker, HierarchicalChunker};
use documents::ParserRegistry;
use embeddings::{EmbeddingProvider, OpenAiProvider, GeminiProvider, NvidiaProvider, EmbeddingInput, LocalHashingSparseEncoder, SparseEmbeddingProvider};

#[tokio::main]
async fn main() {
    // 1. Initialize log tracing
    init_telemetry("ingestion-worker");
    info!("Starting Ingestion Worker daemon...");

    // 2. Load system configurations
    let config = match AppConfig::load_from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load configuration: {:?}", e);
            std::process::exit(1);
        }
    };

    // 3. Initialize Database Connection & Run Migrations
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

    // 4. Set up the embedding provider based on config
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
        warn!("No API keys found in environmental variables. Falling back to local high-fidelity sandbox mock embeddings.");
        Arc::new(NvidiaProvider::new("mock".to_string()))
    };

    // 5. Initialize pipeline components
    let loader = FileLoader::new();
    let chunker = HierarchicalChunker::default();
    let parser_registry = ParserRegistry::new();
    let sparse_encoder = Arc::new(LocalHashingSparseEncoder::default());

    // 6. Initialize Qdrant Connector
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

    // Ensure collection exists (using 1024 dims for NVIDIA nv-embedqa-e5-v5)
    let collection_name = "knowledge_base";
    if let Err(e) = qdrant_client.ensure_collection(collection_name, 1024).await {
        error!("Failed to ensure Qdrant collection: {:?}", e);
    }

    // 7. Worker Ingestion Loop
    info!("Ingestion Worker listening for pending document jobs...");

    loop {
        // Find pending jobs
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

        for job in pending_jobs {
            let tenant = TenantId(job.tenant_id.clone());
            let doc_id = DocumentId(job.id.to_string());
            
            info!(
                tenant = tenant.0,
                document = doc_id.0,
                title = job.title,
                "Processing document job..."
            );

            // Update status to processing
            let mut active_job: document_job::ActiveModel = job.clone().into();
            active_job.status = ActiveValue::set("processing".to_string());
            match active_job.update(&db).await {
                Ok(_) => (),
                Err(e) => {
                    error!(job_id = job.id.to_string(), "Failed to update job status to processing: {:?}", e);
                    continue;
                }
            }

            // STAGE 1: Extract/Load
            let pipeline_result = (|| async {
                // Determine raw bytes (handle base64 if it's a binary file)
                let raw_bytes = if job.file_extension.as_deref() == Some("pdf") || job.file_extension.as_deref() == Some("docx") {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.decode(&job.content).map_err(|e| format!("Base64 decode failed: {:?}", e))?
                } else {
                    job.content.as_bytes().to_vec()
                };

                // Use Parser Registry if an extension is provided, otherwise fallback to plain text
                let extracted_content = if let Some(ext) = &job.file_extension {
                    match parser_registry.get_parser(ext) {
                        Some(parser) => parser.parse(&raw_bytes).map_err(|e| format!("Parser failed: {:?}", e))?,
                        None => {
                            warn!(extension = ext, "No specialized parser found, attempting plain text fallback.");
                            String::from_utf8(raw_bytes).map_err(|e| format!("UTF8 conversion failed: {:?}", e))?
                        }
                    }
                } else {
                    String::from_utf8(raw_bytes).map_err(|e| format!("UTF8 conversion failed: {:?}", e))?
                };

                let doc = loader.load(doc_id.clone(), tenant.clone(), extracted_content.as_bytes(), job.title.clone())
                    .map_err(|e| format!("Loader failed: {:?}", e))?;
                
                info!(job_id = job.id.to_string(), "Stage 1/4 (Extraction) - SUCCESS");

                // STAGE 2: Chunking
                let chunks = chunker.chunk(&doc).map_err(|e| format!("Chunking failed: {:?}", e))?;
                info!(job_id = job.id.to_string(), count = chunks.len(), "Stage 2/4 (Chunking) - SUCCESS");

                // STAGE 3 & 4: Embedding and Storage
                for chunk in &chunks {
                    let input = EmbeddingInput {
                        text: chunk.content.clone(),
                        user_id: None,
                    };
                    
                    let dense_vector = embedding_provider.embed(&input).await
                        .map_err(|e| format!("Dense embedding failed: {:?}", e))?;

                    let sparse_vector = sparse_encoder.embed_sparse(&chunk.content).await
                        .map_err(|e| format!("Sparse embedding failed: {:?}", e))?;

                    qdrant_client.upsert_chunks_hybrid(
                        collection_name,
                        &[chunk.clone()],
                        &[dense_vector.vector],
                        &[(sparse_vector.indices, sparse_vector.values)],
                    ).await.map_err(|e| format!("Storage failed: {:?}", e))?;
                }

                Ok::<(), String>(())
            })().await;

            // Update final status
            let mut final_job: document_job::ActiveModel = job.clone().into();
            match pipeline_result {
                Ok(_) => {
                    final_job.status = ActiveValue::set("completed".to_string());
                    let _ = final_job.update(&db).await;
                    info!(job_id = job.id.to_string(), "Job completed successfully.");
                }
                Err(e) => {
                    error!(job_id = job.id.to_string(), "Job failed: {}", e);
                    final_job.status = ActiveValue::set("failed".to_string());
                    let _ = final_job.update(&db).await;
                }
            }
        }

        // Sleep to throttle poll duration
        sleep(Duration::from_secs(2)).await;
    }
}
