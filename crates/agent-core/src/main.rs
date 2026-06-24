use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    routing::{get, delete, post},
    Json, Router,
};
use rig::{
    agent::MultiTurnStreamItem,
    client::{CompletionClient, ProviderClient},
    completion::{message::ReasoningContent, request::Prompt},
    providers::gemini,
    streaming::{StreamedAssistantContent, StreamingPrompt},
    tool::{Tool, ToolError},
};
use rig::completion::ToolDefinition;
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use search::{CohereReranker, HybridRetriever, SearchService};
use embeddings::providers::NvidiaProvider;
use embeddings::sparse::BM25SparseEncoder;
use connectors::QdrantClient;
use common::config::AppConfig;
use common::telemetry::init_telemetry;
use common::types::TenantId;
use dotenvy::dotenv;
use schemars::JsonSchema;
use futures_util::Stream;
use async_stream::stream;
use tracing::{info, warn};

mod session_store;
use session_store::{SessionStore, ChatMessage, SessionSummary};

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

#[derive(Clone)]
struct AppState {
    search_service: Arc<SearchService>,
    agent: Arc<rig::agent::Agent<gemini::CompletionModel>>,
    sessions: SessionStore,
}

#[derive(Deserialize)]
struct AskRequest {
    query: String,
    session_id: Option<String>,
}

fn now_iso() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let nanos = dur.subsec_nanos();
    let ts = chrono::DateTime::from_timestamp(secs as i64, nanos)
        .unwrap_or_default();
    ts.to_rfc3339()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    init_telemetry("agent-core");
    let config = AppConfig::load_from_env()?;
    info!("Configuration loaded");

    let embedding_provider = Arc::new(NvidiaProvider::new(config.nvidia_api_key.unwrap_or_default()));
    let bm25_stats_path = std::env::var("BM25_STATS_PATH")
        .unwrap_or_else(|_| "./data/bm25_stats.json".to_string());
    let sparse_provider = Arc::new(BM25SparseEncoder::with_persistence(bm25_stats_path));
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

    let sessions = SessionStore::new(&config.redis_url).await?;
    info!(redis_url = config.redis_url, "Redis session store initialized");

    let state = AppState {
        search_service,
        agent,
        sessions,
    };

    let app = Router::new()
        .route("/ask", post(ask_handler))
        .route("/ask_sync", post(ask_sync_handler))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}/messages", get(get_session_messages))
        .route("/sessions/{id}", delete(delete_session))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8001").await?;
    println!("Agent-Core running on http://localhost:8001");
    info!("Listening on http://0.0.0.0:8001");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn list_sessions(
    State(state): State<AppState>,
) -> Json<Vec<SessionSummary>> {
    match state.sessions.list_sessions().await {
        Ok(summaries) => Json(summaries),
        Err(e) => {
            warn!(error = %e, "Failed to list sessions");
            Json(vec![])
        }
    }
}

async fn get_session_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Vec<ChatMessage>> {
    match state.sessions.get_messages(&id).await {
        Ok(messages) => Json(messages),
        Err(e) => {
            warn!(error = %e, session = %id, "Failed to get session messages");
            Json(vec![])
        }
    }
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.sessions.delete_session(&id).await {
        Ok(true) => Json(serde_json::json!({"success": true})),
        Ok(false) => Json(serde_json::json!({"success": false, "error": "Session not found"})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e.to_string()})),
    }
}

async fn ask_handler(
    State(state): State<AppState>,
    Json(payload): Json<AskRequest>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let session_id = payload.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    info!(query = payload.query, session = session_id, "ask_handler called");

    let user_msg = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: payload.query.clone(),
        timestamp: now_iso(),
    };

    let context_preamble = match state.sessions.push_message_and_get_context(&session_id, &user_msg).await {
        Ok(ctx) => ctx,
        Err(e) => {
            warn!(error = %e, "Failed to store user message, continuing without context");
            String::new()
        }
    };

    let full_query = if context_preamble.is_empty() {
        payload.query.clone()
    } else {
        format!("{}\n\nNew question: {}", context_preamble, payload.query)
    };

    let agent = state.agent.clone();
    let sessions = state.sessions.clone();

    let mut agent_stream = agent.stream_prompt(&full_query).await;
    info!("agent stream_prompt returned, starting SSE stream");

    let sse_stream = stream! {
        let mut chunk_count = 0u64;
        let mut final_answer = String::new();

        while let Some(chunk) = agent_stream.next().await {
            match chunk {
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text))) => {
                    chunk_count += 1;
                    final_answer.push_str(&text.text);
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
                    yield Ok(Event::default()
                        .event("reasoning")
                        .data(reasoning_text));
                }
                Ok(MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Final(_))) => {}
                Ok(MultiTurnStreamItem::FinalResponse(response)) => {
                    let final_text = response.response();
                    if final_answer.is_empty() {
                        final_answer = final_text.to_string();
                    }
                    yield Ok(Event::default()
                        .event("final")
                        .data(final_text.to_string()));
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(error = %e, "stream error");
                }
            }
        }

        // Store assistant message
        if !final_answer.is_empty() {
            let assistant_msg = ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content: final_answer,
                timestamp: now_iso(),
            };
            if let Err(e) = sessions.push_message(&session_id, &assistant_msg).await {
                warn!(error = %e, "Failed to store assistant message");
            }
        }

        info!(total_chunks = chunk_count, "SSE stream complete");
    };

    Sse::new(sse_stream)
}

#[derive(Serialize)]
struct SyncAnswer {
    answer: String,
    session_id: String,
}

async fn ask_sync_handler(
    State(state): State<AppState>,
    Json(payload): Json<AskRequest>,
) -> Json<SyncAnswer> {
    let session_id = payload.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    info!(query = payload.query, session = session_id, "ask_sync_handler called");

    let user_msg = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: payload.query.clone(),
        timestamp: now_iso(),
    };

    let context_preamble = match state.sessions.push_message_and_get_context(&session_id, &user_msg).await {
        Ok(ctx) => ctx,
        Err(e) => {
            warn!(error = %e, "Failed to store user message, continuing without context");
            String::new()
        }
    };

    let full_query = if context_preamble.is_empty() {
        payload.query.clone()
    } else {
        format!("{}\n\nNew question: {}", context_preamble, payload.query)
    };

    let agent = state.agent.clone();
    let response = agent.prompt(&full_query).await;

    match response {
        Ok(answer) => {
            info!(answer_len = answer.len(), "sync answer received");
            let assistant_msg = ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content: answer.clone(),
                timestamp: now_iso(),
            };
            if let Err(e) = state.sessions.push_message(&session_id, &assistant_msg).await {
                warn!(error = %e, "Failed to store assistant message");
            }
            Json(SyncAnswer { answer, session_id })
        }
        Err(e) => {
            warn!(error = %e, "sync prompt error");
            Json(SyncAnswer { answer: format!("Error: {}", e), session_id })
        }
    }
}
