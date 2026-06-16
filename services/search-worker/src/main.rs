use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

use common::config::AppConfig;
use common::telemetry::init_telemetry;
use common::types::TenantId;
use connectors::{GraphClient, QdrantClient};
use embeddings::{EmbeddingProvider, LocalHashingSparseEncoder, NvidiaProvider};
use llm::{GeminiLlm, LlmProvider, NvidiaLlm, OpenAiLlm, RagService};
use search::retrievers::{Retriever, SearchConfig, SearchResult};
use search::{GraphRetriever, HybridRetriever};

#[derive(Deserialize)]
struct SearchParams {
    query: String,
    limit: Option<usize>,
    tenant_id: Option<String>,
    rrf_k: Option<f32>,
    dense_weight: Option<f32>,
    sparse_weight: Option<f32>,
    entity_weight: Option<f32>,
    graph_weight: Option<f32>,
}

#[derive(Deserialize)]
struct AskParams {
    question: String,
    tenant_id: Option<String>,
}

struct AppState {
    retriever: Arc<dyn Retriever>,
    rag_service: Arc<RagService>,
    qdrant_client: Arc<QdrantClient>,
    collection_name: String,
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

    // 4. Setup Graph Client (Postgres for knowledge graph traversal)
    let graph_client = Arc::new(
        GraphClient::new(&config.database_url).await
            .expect("Failed to connect to Postgres for graph queries")
    );

    // 5. Setup Graph Retriever
    let graph_retriever = Arc::new(GraphRetriever::new(
        graph_client,
        qdrant_client.clone(),
        embedding_provider.clone(),
        "knowledge_base".to_string(),
    ));

    // 6. Setup Retriever (hybrid with graph support)
    let sparse_provider = Arc::new(LocalHashingSparseEncoder::default());
    let retriever = Arc::new(HybridRetriever::with_graph_retriever(
        embedding_provider,
        sparse_provider,
        qdrant_client.clone(),
        "knowledge_base".to_string(),
        graph_retriever,
    ));

    // 7. Setup RAG Service
    let rag_service = Arc::new(RagService::new(retriever.clone(), llm_provider));

    let state = Arc::new(AppState { retriever, rag_service, qdrant_client, collection_name: "knowledge_base".to_string() });

    // 8. Build router
    let app = Router::new()
        .route("/search", get(search_handler))
        .route("/ask", get(ask_handler))
        .route("/documents/{id}", delete(delete_document_handler))
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
    let config = SearchConfig {
        rrf_k: params.rrf_k.unwrap_or(60.0),
        dense_weight: params.dense_weight.unwrap_or(1.0),
        sparse_weight: params.sparse_weight.unwrap_or(1.0),
        entity_weight: params.entity_weight.unwrap_or(0.8),
        graph_weight: params.graph_weight.unwrap_or(0.6),
    };

    match state.retriever.retrieve_with_config(&tenant_id, &params.query, limit, &config).await {
        Ok(results) => Json(results),
        Err(e) => {
            error!("Search failed: {:?}", e);
            Json(vec![])
        }
    }
}

#[derive(Serialize)]
struct DeleteResponse {
    success: bool,
    message: String,
}

async fn delete_document_handler(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Result<Json<DeleteResponse>, StatusCode> {
    match state
        .qdrant_client
        .delete_points_by_document_id(&state.collection_name, &document_id)
        .await
    {
        Ok(_) => Ok(Json(DeleteResponse {
            success: true,
            message: format!("Deleted chunks for document {}", document_id),
        })),
        Err(e) => {
            error!("Failed to delete document from Qdrant: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
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
