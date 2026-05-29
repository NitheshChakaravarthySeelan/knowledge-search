use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use common::errors::{AppError, Result};
use crate::LlmProvider;

pub struct OpenAiLlm {
    pub api_key: String,
    client: Client,
}

impl OpenAiLlm {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiLlm {
    async fn generate(&self, prompt: &str, system_instruction: Option<&str>) -> Result<String> {
        if self.api_key.is_empty() || self.api_key == "mock" {
            return Ok(generate_mock_response(prompt));
        }

        let mut messages = Vec::new();
        if let Some(sys) = system_instruction {
            messages.push(json!({ "role": "system", "content": sys }));
        }
        messages.push(json!({ "role": "user", "content": prompt }));

        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": "gpt-4o-mini",
                "messages": messages,
                "temperature": 0.2
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "OpenAI LLM".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalService {
                service: "OpenAI LLM".to_string(),
                message: format!("Status: {}, Body: {}", status, body),
            });
        }

        let json_body: serde_json::Value = response.json().await.map_err(|e| AppError::ExternalService {
            service: "OpenAI LLM".to_string(),
            message: format!("Failed to parse response body as JSON: {}", e),
        })?;
        let text = json_body["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AppError::ExternalService {
                service: "OpenAI LLM".to_string(),
                message: "Missing completion text".to_string(),
            })?
            .to_string();

        Ok(text)
    }
}

pub struct GeminiLlm {
    pub api_key: String,
    client: Client,
}

impl GeminiLlm {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiLlm {
    async fn generate(&self, prompt: &str, system_instruction: Option<&str>) -> Result<String> {
        if self.api_key.is_empty() || self.api_key == "mock" {
            return Ok(generate_mock_response(prompt));
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent?key={}",
            self.api_key
        );

        let contents = json!({
            "parts": [{ "text": prompt }]
        });
        
        let mut request_payload = json!({ "contents": [contents] });
        
        if let Some(sys) = system_instruction {
            request_payload["systemInstruction"] = json!({
                "parts": [{ "text": sys }]
            });
        }

        let response = self.client
            .post(&url)
            .json(&request_payload)
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Gemini LLM".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalService {
                service: "Gemini LLM".to_string(),
                message: format!("Status: {}, Body: {}", status, body),
            });
        }

        let json_body: serde_json::Value = response.json().await.map_err(|e| AppError::ExternalService {
            service: "Gemini LLM".to_string(),
            message: format!("Failed to parse response body as JSON: {}", e),
        })?;
        let text = json_body["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| AppError::ExternalService {
                service: "Gemini LLM".to_string(),
                message: "Missing generation text in Gemini response".to_string(),
            })?
            .to_string();

        Ok(text)
    }
}

/// Generates a clean, mock markdown response summarizing query information.
fn generate_mock_response(prompt: &str) -> String {
    format!(
        "### Core Analysis Summary (Local Mock LLM)\n\n\
        I processed your requested query. Based on semantic indexing, here are the key insights:\n\n\
        1. **Query Alignment**: The system successfully analyzed your prompt: *\"{}\"*.\n\
        2. **Semantic Density**: We identified key concepts matching your structural definitions.\n\
        3. **Synthesis**: The infrastructure monorepo is operational, compiling perfectly on Rust/Bun.\n\n\
        > [!NOTE]\n\
        > This answer was generated locally using the high-fidelity mock compiler fallback. Ensure your API keys are added in `APP_ENV` for real LLM operations.",
        prompt.chars().take(60).collect::<String>()
    )
}
