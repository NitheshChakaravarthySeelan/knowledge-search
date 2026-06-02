use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error};
use sea_orm::{Database, DatabaseConnection, EntityTrait, QueryFilter, ColumnTrait, ActiveModelTrait, ActiveValue};

use common::config::AppConfig;
use common::telemetry::init_telemetry;
use common::types::{DocumentId, TenantId};
use common::migrations::Migrator;
use common::entities::document_job;
use sea_orm_migration::prelude::*;
use sea_orm_migration::migrator::MigratorTrait;

use connectors::QdrantClient;
use documents::loaders::{Loader, FileLoader};
use documents::chunkers::{Chunker, RecursiveTextChunker};
use embeddings::{EmbeddingProvider, OpenAiProvider, GeminiProvider, NvidiaProvider, EmbeddingInput};

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
    let chunker = RecursiveTextChunker::default();

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
            let pipeline_result = match loader.load(doc_id.clone(), tenant.clone(), job.content.as_bytes(), job.title.clone()) {
                Ok(doc) => {
                    info!(job_id = job.id.to_string(), "Stage 1/4 (Extraction) - SUCCESS");

                    // STAGE 2: Chunking
                    match chunker.chunk(&doc) {
                        Ok(chunks) => {
                            info!(job_id = job.id.to_string(), count = chunks.len(), "Stage 2/4 (Chunking) - SUCCESS");

                            // STAGE 3 & 4: Embedding and Storage
                            let mut pipeline_success = true;
                            for chunk in &chunks {
                                let input = EmbeddingInput {
                                    text: chunk.content.clone(),
                                    user_id: None,
                                };
                                
                                match embedding_provider.embed(&input).await {
                                    Ok(vector) => {
                                        // STAGE 4: Storage
                                        if let Err(e) = qdrant_client.upsert_chunks(
                                            collection_name,
                                            &[chunk.clone()],
                                            &[vector.vector],
                                        ).await {
                                            error!(job_id = job.id.to_string(), chunk_id = chunk.id, "Stage 4/4 (Storage) - FAILED: {:?}", e);
                                            pipeline_success = false;
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!(job_id = job.id.to_string(), chunk_id = chunk.id, "Stage 3/4 (Embedding) - FAILED: {:?}", e);
                                        pipeline_success = false;
                                        break;
                                    }
                                }
                            }

                            if pipeline_success {
                                Ok(())
                            } else {
                                Err("Pipeline failed during embedding or storage".to_string())
                            }
                        }
                        Err(e) => {
                            error!(job_id = job.id.to_string(), "Stage 2/4 (Chunking) - FAILED: {:?}", e);
                            Err(format!("Chunking failed: {:?}", e))
                        }
                    }
                }
                Err(e) => {
                    error!(job_id = job.id.to_string(), "Stage 1/4 (Extraction) - FAILED: {:?}", e);
                    Err(format!("Extraction failed: {:?}", e))
                }
            };

            // Update final status
            let mut final_job: document_job::ActiveModel = job.clone().into();
            match pipeline_result {
                Ok(_) => {
                    final_job.status = ActiveValue::set("completed".to_string());
                    let _ = final_job.update(&db).await;
                    info!(job_id = job.id.to_string(), "Job completed successfully.");
                }
                Err(_) => {
                    final_job.status = ActiveValue::set("failed".to_string());
                    let _ = final_job.update(&db).await;
                }
            }
        }

        // Sleep to throttle poll duration
        sleep(Duration::from_secs(2)).await;
    }
}
