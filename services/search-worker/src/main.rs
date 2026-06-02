use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, error};

use common::config::AppConfig;
use common::telemetry::init_telemetry;
use common::types::TenantId;
use connectors::QdrantClient;
use embeddings::{NvidiaProvider, GeminiProvider, OpenAiProvider, EmbeddingProvider};
use search::retrievers::{Retriever, VectorRetriever, SearchResult};

#[derive(Deserialize)]
struct SearchParams {
    query: String,
    limit: Option<usize>,
    tenant_id: Option<String>,
}

struct AppState {
    retriever: Arc<dyn Retriever>,
}

#[tokio::main]
async fn main() {
    init_telemetry("search-worker");
    info!("Starting Search Worker API...");

    let config = AppConfig::load_from_env().expect("Failed to load config");

    // 1. Setup Embedding Provider (NVIDIA priority)
    let embedding_provider: Arc<dyn EmbeddingProvider> = if let Some(key) = &config.nvidia_api_key {
        Arc::new(NvidiaProvider::new(key.clone()))
    } else if let Some(key) = &config.gemini_api_key {
        Arc::new(GeminiProvider::new(key.clone()))
    } else if let Some(key) = &config.openai_api_key {
        Arc::new(OpenAiProvider::new(key.clone()))
    } else {
        Arc::new(NvidiaProvider::new("mock".to_string()))
    };

    // 2. Setup Qdrant Client
    let qdrant_client = Arc::new(QdrantClient::new(&config.qdrant_url).expect("Failed to connect to Qdrant"));

    // 3. Setup Retriever
    let retriever = Arc::new(VectorRetriever::new(
        embedding_provider,
        qdrant_client,
        "knowledge_base".to_string(),
    ));

    let state = Arc::new(AppState { retriever });

    // 4. Build router
    let app = Router::new()
        .route("/search", get(search_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8081").await.unwrap();
    info!("Search Worker listening on http://0.0.0.0:8081");
    axum::serve(listener, app).await.unwrap();
}

async fn search_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<Vec<SearchResult>> {
    let tenant_id = TenantId(params.tenant_id.unwrap_or_else(|| "tenant_corporate_1".to_string()));
    let limit = params.limit.unwrap_or(10);

    match state.retriever.retrieve(&tenant_id, &params.query, limit).await {
        Ok(results) => Json(results),
        Err(e) => {
            error!("Search failed: {:?}", e);
            Json(vec![])
        }
    }
}
