use std::collections::HashMap;
use std::sync::Arc;
use std::convert::Infallible;
use tokio::sync::Mutex;

use anyhow::Result;
use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    routing::{get, delete, post},
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
use embeddings::sparse::BM25SparseEncoder;
use connectors::QdrantClient;
use common::config::AppConfig;
use common::telemetry::init_telemetry;
use common::types::TenantId;
use dotenvy::dotenv;
use schemars::JsonSchema;
use futures_util::{Stream, StreamExt};
use async_stream::stream;
use tracing::{info, warn};

const MAX_SESSION_HISTORY: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    id: String,
    role: String,
    content: String,
    timestamp: String,
}

#[derive(Debug, Serialize)]
struct SessionSummary {
    id: String,
    preview: String,
    message_count: usize,
    last_timestamp: String,
}

#[derive(Clone)]
struct AppState {
    search_service: Arc<SearchService>,
    agent: Arc<rig::agent::Agent<gemini::CompletionModel>>,
    sessions: Arc<Mutex<HashMap<String, Vec<ChatMessage>>>>,
}

#[derive(Deserialize)]
struct AskRequest {
    query: String,
    session_id: Option<String>,
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

fn build_conversation_context(messages: &[ChatMessage]) -> String {
    let history: Vec<&ChatMessage> = messages.iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .collect();

    if history.is_empty() {
        return String::new();
    }

    let mut context = String::from("\n\n--- Previous conversation ---\n");
    for msg in &history {
        let label = if msg.role == "user" { "User" } else { "Assistant" };
        context.push_str(&format!("{}: {}\n\n", label, msg.content));
    }
    context.push_str("--- End of previous conversation ---\n");
    context
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

    let state = AppState {
        search_service,
        agent,
        sessions: Arc::new(Mutex::new(HashMap::new())),
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
    let sessions = state.sessions.lock().await;
    let mut summaries: Vec<SessionSummary> = sessions.iter()
        .map(|(id, messages)| {
            let preview = messages.iter()
                .find(|m| m.role == "user")
                .map(|m| {
                    let truncated: String = m.content.chars().take(80).collect();
                    if m.content.len() > 80 { format!("{}...", truncated) } else { truncated }
                })
                .unwrap_or_default();

            let last_ts = messages.last()
                .map(|m| m.timestamp.clone())
                .unwrap_or_default();

            SessionSummary {
                id: id.clone(),
                preview,
                message_count: messages.len(),
                last_timestamp: last_ts,
            }
        })
        .collect();

    summaries.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
    Json(summaries)
}

async fn get_session_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Vec<ChatMessage>> {
    let sessions = state.sessions.lock().await;
    let messages = sessions.get(&id).cloned().unwrap_or_default();
    Json(messages)
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let mut sessions = state.sessions.lock().await;
    if sessions.remove(&id).is_some() {
        Json(serde_json::json!({"success": true}))
    } else {
        Json(serde_json::json!({"success": false, "error": "Session not found"}))
    }
}

async fn ask_handler(
    State(state): State<AppState>,
    Json(payload): Json<AskRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = payload.session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    info!(query = payload.query, session = session_id, "ask_handler called");

    // Append user message to history
    let user_msg = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: payload.query.clone(),
        timestamp: now_iso(),
    };

    let (context_preamble, sid) = {
        let mut sessions = state.sessions.lock().await;
        let history = sessions.entry(session_id.clone()).or_default();
        let ctx = build_conversation_context(history);
        history.push(user_msg);
        // Trim oldest pairs if over limit
        while history.len() > MAX_SESSION_HISTORY * 2 {
            history.remove(0);
        }
        (ctx, session_id.clone())
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
            let mut sessions = sessions.lock().await;
            if let Some(history) = sessions.get_mut(&sid) {
                history.push(ChatMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    role: "assistant".to_string(),
                    content: final_answer,
                    timestamp: now_iso(),
                });
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

    // Append user message
    let user_msg = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: payload.query.clone(),
        timestamp: now_iso(),
    };

    let context_preamble = {
        let mut sessions = state.sessions.lock().await;
        let history = sessions.entry(session_id.clone()).or_default();
        let ctx = build_conversation_context(history);
        history.push(user_msg);
        while history.len() > MAX_SESSION_HISTORY * 2 {
            history.remove(0);
        }
        ctx
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
            let mut sessions = state.sessions.lock().await;
            if let Some(history) = sessions.get_mut(&session_id) {
                history.push(ChatMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    role: "assistant".to_string(),
                    content: answer.clone(),
                    timestamp: now_iso(),
                });
            }
            Json(SyncAnswer { answer, session_id })
        }
        Err(e) => {
            warn!(error = %e, "sync prompt error");
            Json(SyncAnswer { answer: format!("Error: {}", e), session_id })
        }
    }
}
