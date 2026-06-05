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
use embeddings::{NvidiaProvider, GeminiProvider, OpenAiProvider, EmbeddingProvider, LocalHashingSparseEncoder};
use llm::{NvidiaLlm, GeminiLlm, OpenAiLlm, LlmProvider, RagService};
use search::retrievers::{Retriever, SearchResult};
use search::HybridRetriever;

#[derive(Deserialize)]
struct SearchParams {
    query: String,
    limit: Option<usize>,
    tenant_id: Option<String>,
}

#[derive(Deserialize)]
struct AskParams {
    question: String,
    tenant_id: Option<String>,
}

struct AppState {
    retriever: Arc<dyn Retriever>,
    rag_service: Arc<RagService>,
}

#[tokio::main]
async fn main() {
    init_telemetry("search-worker");
    info!("Starting Search Worker API...");

    let config = AppConfig::load_from_env().expect("Failed to load config");

    // 1. Setup Embedding Provider (NVIDIA priority)
    let embedding_provider: Arc<dyn EmbeddingProvider> = if let Some(key) = &config.nvidia_api_key {
        Arc::new(NvidiaProvider::new(key.clone()))
    } else {
        Arc::new(NvidiaProvider::new("mock".to_string()))
    };

    // 2. Setup LLM Provider (NVIDIA priority)
    let llm_provider: Arc<dyn LlmProvider> = if let Some(key) = &config.nvidia_api_key {
        Arc::new(NvidiaLlm::new(key.clone()))
    } else if let Some(key) = &config.gemini_api_key {
        Arc::new(GeminiLlm::new(key.clone()))
    } else if let Some(key) = &config.openai_api_key {
        Arc::new(OpenAiLlm::new(key.clone()))
    } else {
        Arc::new(NvidiaLlm::new("mock".to_string()))
    };

    // 3. Setup Qdrant Client
    let qdrant_client = Arc::new(QdrantClient::new(&config.qdrant_url).expect("Failed to connect to Qdrant"));

    // 4. Setup Retriever
    let sparse_provider = Arc::new(LocalHashingSparseEncoder::default());
    let retriever = Arc::new(HybridRetriever::new(
        embedding_provider,
        sparse_provider,
        qdrant_client,
        "knowledge_base".to_string(),
    ));

    // 5. Setup RAG Service
    let rag_service = Arc::new(RagService::new(retriever.clone(), llm_provider));

    let state = Arc::new(AppState { retriever, rag_service });

    // 6. Build router
    let app = Router::new()
        .route("/search", get(search_handler))
        .route("/ask", get(ask_handler))
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

async fn ask_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AskParams>,
) -> Json<serde_json::Value> {
    let tenant_id = TenantId(params.tenant_id.unwrap_or_else(|| "tenant_corporate_1".to_string()));

    match state.rag_service.ask(&tenant_id, &params.question).await {
        Ok(answer) => Json(serde_json::json!({ "answer": answer })),
        Err(e) => {
            error!("RAG Ask failed: {:?}", e);
            Json(serde_json::json!({ "answer": "I encountered an error while processing your request.", "error": e.to_string() }))
        }
    }
}
