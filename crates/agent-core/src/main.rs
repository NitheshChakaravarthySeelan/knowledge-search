use anyhow::{anyhow, Result};
use axum::{routing::post, Json, Router, extract::State};
use serde::{Deserialize, Serialize};
use rig::tool::{Tool, ToolError};
use rig::completion::{Prompt, ToolDefinition};
use rig::providers::gemini;
use rig::client::{ProviderClient, CompletionClient};
use search::{SearchService, HybridRetriever, CohereReranker};
use embeddings::providers::NvidiaProvider;
use embeddings::sparse::LocalHashingSparseEncoder;
use connectors::QdrantClient;
use common::config::AppConfig;
use common::types::TenantId;
use std::sync::Arc;
use dotenvy::dotenv;
use schemars::JsonSchema;

#[derive(Clone)]
struct AppState {
    search_service: Arc<SearchService>,
    agent: Arc<rig::agent::Agent<gemini::completion::CompletionModel>>,
}

#[derive(Deserialize)]
struct AskRequest {
    query: String,
}

#[derive(Serialize)]
struct AskResponse {
    answer: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchArgs {
    query: String,
    tenant_id: Option<String>,
    limit: Option<u64>,
}

struct KnowledgeBaseTool {
    search_service: Arc<SearchService>,
}

impl Tool for KnowledgeBaseTool {
    const NAME: &'static str = "search_knowledge_base";
    type Args = SearchArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Queries and searches the local knowledge base for contextual documentation.".to_string(),
            parameters: schemars::schema_for!(SearchArgs).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let tenant_id_str = args.tenant_id.unwrap_or_else(|| "default".to_string());
        let tenant = TenantId(tenant_id_str);
        let limit = args.limit.unwrap_or(5) as usize;

        let results = self.search_service.search(&tenant, &args.query, limit).await
            .map_err(|e| ToolError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>))?;
            
        Ok(serde_json::to_string(&results).map_err(|e| ToolError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>))?)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    let config = AppConfig::load_from_env()?;
    
    let embedding_provider = Arc::new(NvidiaProvider::new(config.nvidia_api_key.unwrap_or_default()));
    let sparse_provider = Arc::new(LocalHashingSparseEncoder::default());
    let qdrant_client = Arc::new(QdrantClient::new(&config.qdrant_url)?);
    
    let retriever = Arc::new(HybridRetriever::new(
        embedding_provider,
        sparse_provider,
        qdrant_client,
        "knowledge_base".to_string(),
    ));

    let reranker = Arc::new(CohereReranker::new(config.cohere_api_key.unwrap_or_default()));
    let search_service = Arc::new(SearchService::new(retriever, reranker));

    // Fix: Unwrapped client initialization
    let gemini_client = gemini::Client::from_env().expect("Failed to initialize Gemini client");
    
    let kb_tool = KnowledgeBaseTool { search_service: search_service.clone() };
    
    let agent = Arc::new(
        gemini_client
            .agent("gemma-4-31b-it")
            .preamble("You are a helpful knowledge base agent. Always use search_knowledge_base tool.")
            .tool(kb_tool)
            .build(),
    );

    let state = AppState { search_service, agent };

    let app = Router::new()
        .route("/ask", post(ask_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8001").await?;
    println!("Agent-Core running on http://localhost:8001");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn ask_handler(
    State(state): State<AppState>,
    Json(payload): Json<AskRequest>,
) -> Json<AskResponse> {
    let response = state.agent.prompt(&payload.query).await.unwrap_or_else(|_| "Agent error".into());
    Json(AskResponse { answer: response.to_string() })
}
