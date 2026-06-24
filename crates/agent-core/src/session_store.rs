use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use anyhow::Result;
use serde::{Deserialize, Serialize};

const SESSION_TTL_SECS: u64 = 86400; // 24 hours
const KEY_PREFIX: &str = "session:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub preview: String,
    pub message_count: usize,
    pub last_timestamp: String,
}

#[derive(Clone)]
pub struct SessionStore {
    conn: ConnectionManager,
}

impl SessionStore {
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(Self { conn })
    }

    fn key(session_id: &str) -> String {
        format!("{}{}", KEY_PREFIX, session_id)
    }

    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>> {
        let key = Self::key(session_id);
        let mut conn = self.conn.clone();
        let data: Option<String> = conn.get(&key).await?;
        match data {
            Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            None => Ok(vec![]),
        }
    }

    pub async fn push_message(&self, session_id: &str, msg: &ChatMessage) -> Result<()> {
        let mut messages = self.get_messages(session_id).await?;
        messages.push(msg.clone());
        // Trim oldest if over limit
        const MAX_HISTORY: usize = 100;
        if messages.len() > MAX_HISTORY {
            messages.drain(0..messages.len() - MAX_HISTORY);
        }
        let key = Self::key(session_id);
        let json = serde_json::to_string(&messages)?;
        let mut conn = self.conn.clone();
        let _: () = conn.set_ex(&key, &json, SESSION_TTL_SECS).await?;
        Ok(())
    }

    pub async fn push_message_and_get_context(
        &self,
        session_id: &str,
        msg: &ChatMessage,
    ) -> Result<String> {
        // Get existing messages (before pushing the new one)
        let messages = self.get_messages(session_id).await?;
        let context = build_conversation_context(&messages);
        // Push the new message
        let mut new_messages = messages;
        new_messages.push(msg.clone());
        const MAX_HISTORY: usize = 100;
        if new_messages.len() > MAX_HISTORY {
            new_messages.drain(0..new_messages.len() - MAX_HISTORY);
        }
        let key = Self::key(session_id);
        let json = serde_json::to_string(&new_messages)?;
        let mut conn = self.conn.clone();
        let _: () = conn.set_ex(&key, &json, SESSION_TTL_SECS).await?;
        Ok(context)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool> {
        let key = Self::key(session_id);
        let mut conn = self.conn.clone();
        let deleted: usize = conn.del(&key).await?;
        Ok(deleted > 0)
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        let mut summaries = Vec::new();
        let mut cursor = 0isize;
        let mut conn = self.conn.clone();

        loop {
            let (next_cursor, keys): (isize, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(format!("{}*", KEY_PREFIX))
                .arg("COUNT")
                .arg(100usize)
                .query_async(&mut conn)
                .await?;

            for key in &keys {
                if let Some(summary) = self.build_summary_from_key(key).await? {
                    summaries.push(summary);
                }
            }

            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        summaries.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
        Ok(summaries)
    }

    async fn build_summary_from_key(&self, key: &str) -> Result<Option<SessionSummary>> {
        let session_id = key.strip_prefix(KEY_PREFIX).unwrap_or(key).to_string();
        let mut conn = self.conn.clone();
        let data: Option<String> = conn.get(key).await?;

        match data {
            Some(json) => {
                let messages: Vec<ChatMessage> =
                    serde_json::from_str(&json).unwrap_or_default();
                if messages.is_empty() {
                    return Ok(None);
                }
                let preview = messages
                    .iter()
                    .find(|m| m.role == "user")
                    .map(|m| {
                        let truncated: String = m.content.chars().take(80).collect();
                        if m.content.len() > 80 {
                            format!("{}...", truncated)
                        } else {
                            truncated
                        }
                    })
                    .unwrap_or_default();
                let last_ts = messages
                    .last()
                    .map(|m| m.timestamp.clone())
                    .unwrap_or_default();
                Ok(Some(SessionSummary {
                    id: session_id,
                    preview,
                    message_count: messages.len(),
                    last_timestamp: last_ts,
                }))
            }
            None => Ok(None),
        }
    }
}

fn build_conversation_context(messages: &[ChatMessage]) -> String {
    let history: Vec<&ChatMessage> = messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .collect();

    if history.is_empty() {
        return String::new();
    }

    let mut context = String::from("\n\n--- Previous conversation ---\n");
    for msg in &history {
        let label = if msg.role == "user" {
            "User"
        } else {
            "Assistant"
        };
        context.push_str(&format!("{}: {}\n\n", label, msg.content));
    }
    context.push_str("--- End of previous conversation ---\n");
    context
}
