use anyhow::Result;
use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use rig::{
    agent::MultiTurnStreamItem,
    client::{CompletionClient, ProviderClient},
    completion::{message::ReasoningContent, request::Prompt, ToolDefinition},
    providers::gemini,
    streaming::{StreamedAssistantContent, StreamingPrompt},
    tool::{Tool, ToolError},
};
use search::{CohereReranker, HybridRetriever, SearchService};
use embeddings::providers::NvidiaProvider;
use embeddings::sparse::LocalHashingSparseEncoder;
use connectors::QdrantClient;
use common::config::AppConfig;
use common::telemetry::init_telemetry;
use common::types::TenantId;
use std::sync::Arc;
use dotenvy::dotenv;
use schemars::JsonSchema;
use futures_util::{Stream, StreamExt};
use std::convert::Infallible;
use async_stream::stream;
use tracing::{info, warn};

#[derive(Clone)]
struct AppState {
    search_service: Arc<SearchService>,
    agent: Arc<rig::agent::Agent<gemini::CompletionModel>>,
}

#[derive(Deserialize)]
struct AskRequest {
    query: String,
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

    async fn call(&self, args: Self::Args) -> Result<String, ToolError> {
        let tenant_id_str = args.tenant_id.unwrap_or_else(|| "default".to_string());
        let tenant = TenantId(tenant_id_str);
        let limit = args.limit.unwrap_or(5) as usize;

        info!(query = args.query, "KnowledgeBaseTool called");
        let results = self.search_service.search(&tenant, &args.query, limit).await
            .map_err(|e| ToolError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>))?;
            
        info!(result_count = results.len(), "KnowledgeBaseTool returned results");
        Ok(serde_json::to_string(&results).map_err(|e| ToolError::from(Box::new(e) as Box<dyn std::error::Error + Send + Sync>))?)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    init_telemetry("agent-core");
    let config = AppConfig::load_from_env()?;
    info!("Configuration loaded");
    
    let embedding_provider = Arc::new(NvidiaProvider::new(config.nvidia_api_key.unwrap_or_default()));
    let sparse_provider = Arc::new(LocalHashingSparseEncoder::default());
    let qdrant_client = Arc::new(QdrantClient::new(&config.qdrant_url)?);
    info!(qdrant_url = config.qdrant_url, "Qdrant client initialized");
    
    let retriever = Arc::new(HybridRetriever::new(
        embedding_provider,
        sparse_provider,
        qdrant_client,
        "knowledge_base".to_string(),
    ));

    let reranker = Arc::new(CohereReranker::new(config.cohere_api_key.unwrap_or_default()));
    let search_service = Arc::new(SearchService::new(retriever, reranker));

    let gemini_client = gemini::Client::from_env().expect("Failed to initialize Gemini client");
    info!("Gemini client initialized");
    
    let kb_tool = KnowledgeBaseTool { search_service: search_service.clone() };
    
    let agent = Arc::new(
        gemini_client
            .agent("gemma-4-31b-it")
            .preamble("You are a professional, expert research assistant. Your responses must be structured, professional, and visually clean using Markdown. Always use clear headers (#), bullet points, and bold text to improve readability. For every piece of information used, YOU MUST CITE THE SOURCE using [Document Title] or [Chunk ID]. Your primary goal is to answer questions using ONLY the information provided in the knowledge base. If the information is not found in the knowledge base, state 'I cannot answer this based on the available information'.")
            .tool(kb_tool)
            .build(),
    );
    info!("Agent built with gemma-4-31b-it");

    let state = AppState { search_service, agent };

    let app = Router::new()
        .route("/ask", post(ask_handler))
        .route("/ask_sync", post(ask_sync_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8001").await?;
    println!("Agent-Core running on http://localhost:8001");
    info!("Listening on http://0.0.0.0:8001");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn ask_handler(
    State(state): State<AppState>,
    Json(payload): Json<AskRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!(query = payload.query, "ask_handler called");
    let agent = state.agent.clone();
    let mut agent_stream = agent.stream_prompt(&payload.query).await;
    info!("agent stream_prompt returned, starting SSE stream");

    let sse_stream = stream! {
        let mut chunk_count = 0u64;
        while let Some(chunk) = agent_stream.next().await {
            match chunk {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text))) => {
                    chunk_count += 1;
                    info!(chunk = chunk_count, text_len = text.text.len(), "yielding Text chunk");
                    yield Ok(Event::default().data(text.text));
                }
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Reasoning(reasoning))) => {
                    let reasoning_text: String = reasoning.content.iter()
                        .filter_map(|c| match c {
                            ReasoningContent::Text { text, .. } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    info!(reasoning_len = reasoning_text.len(), "yielding Reasoning");
                    yield Ok(Event::default()
                        .event("reasoning")
                        .data(reasoning_text));
                }
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Final(_))) => {
                    // Final completion metadata — handled by FinalResponse below
                    info!("stream final item received (metadata only)");
                }
                Ok(MultiTurnStreamItem::FinalResponse(response)) => {
                    let final_text = response.response();
                    info!(response_len = final_text.len(), "yielding FinalResponse");
                    yield Ok(Event::default()
                        .event("final")
                        .data(final_text.to_string()));
                }
                Ok(MultiTurnStreamItem::CompletionCall(call)) => {
                    info!(call_index = call.call_index, "completion call finished");
                }
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ToolCall { tool_call, .. })) => {
                    info!(tool_name = tool_call.function.name, "tool call");
                }
                Ok(_) => {
                    warn!("unhandled stream item variant");
                }
                Err(e) => {
                    warn!(error = %e, "stream error");
                }
            }
        }
        info!(total_chunks = chunk_count, "SSE stream complete");
    };

    Sse::new(sse_stream)
}

#[derive(Serialize)]
struct SyncAnswer {
    answer: String,
}

async fn ask_sync_handler(
    State(state): State<AppState>,
    Json(payload): Json<AskRequest>,
) -> Json<SyncAnswer> {
    info!(query = payload.query, "ask_sync_handler called");
    let agent = state.agent.clone();
    let response = agent.prompt(&payload.query).await;
    match response {
        Ok(answer) => {
            info!(answer_len = answer.len(), "sync answer received");
            Json(SyncAnswer { answer })
        }
        Err(e) => {
            warn!(error = %e, "sync prompt error");
            Json(SyncAnswer { answer: format!("Error: {}", e) })
        }
    }
}
