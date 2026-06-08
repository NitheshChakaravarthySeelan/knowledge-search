use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use common::errors::{AppError, Result};
use crate::models::{Embedding, EmbeddingInput};
use crate::traits::EmbeddingProvider;
use tokio_retry::Retry;
use tokio_retry::strategy::{ExponentialBackoff, jitter};

pub struct OpenAiProvider {
    pub api_key: String,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiProvider {
    async fn embed(&self, input: &EmbeddingInput) -> Result<Embedding> {
        if self.api_key.is_empty() || self.api_key == "mock" {
            return generate_mock_embedding(&input.text, 1536);
        }

        let response = self.client
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&json!({
                "input": input.text,
                "model": "text-embedding-3-small"
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "OpenAI".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalService {
                service: "OpenAI".to_string(),
                message: format!("Status: {}, Body: {}", status, body),
            });
        }

        let json_body: serde_json::Value = response.json().await.map_err(|e| AppError::ExternalService {
            service: "OpenAI".to_string(),
            message: format!("Failed to parse response body as JSON: {}", e),
        })?;
        let vector = json_body["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| AppError::ExternalService {
                service: "OpenAI".to_string(),
                message: "Missing embedding vector in OpenAI response".to_string(),
            })?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();

        Ok(Embedding {
            vector,
            dimensions: 1536,
        })
    }

    async fn embed_batch(&self, inputs: &[EmbeddingInput]) -> Result<Vec<Embedding>> {
        let mut results = Vec::new();
        for input in inputs {
            results.push(self.embed(input).await?);
        }
        Ok(results)
    }
}

pub struct GeminiProvider {
    pub api_key: String,
    client: Client,
}

impl GeminiProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for GeminiProvider {
    async fn embed(&self, input: &EmbeddingInput) -> Result<Embedding> {
        if self.api_key.is_empty() || self.api_key == "mock" {
            return generate_mock_embedding(&input.text, 768);
        }

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent?key={}",
            self.api_key
        );

        let response = self.client
            .post(&url)
            .json(&json!({
                "content": {
                    "parts": [{ "text": input.text }]
                }
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Gemini".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalService {
                service: "Gemini".to_string(),
                message: format!("Status: {}, Body: {}", status, body),
            });
        }

        let json_body: serde_json::Value = response.json().await.map_err(|e| AppError::ExternalService {
            service: "Gemini".to_string(),
            message: format!("Failed to parse response body as JSON: {}", e),
        })?;
        let vector = json_body["embedding"]["values"]
            .as_array()
            .ok_or_else(|| AppError::ExternalService {
                service: "Gemini".to_string(),
                message: "Missing embedding vector in Gemini response".to_string(),
            })?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();

        Ok(Embedding {
            vector,
            dimensions: 768,
        })
    }

    async fn embed_batch(&self, inputs: &[EmbeddingInput]) -> Result<Vec<Embedding>> {
        let mut results = Vec::new();
        for input in inputs {
            results.push(self.embed(input).await?);
        }
        Ok(results)
    }
}

pub struct NvidiaProvider {
    pub api_key: String,
    client: Client,
}

impl NvidiaProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for NvidiaProvider {
    async fn embed(&self, input: &EmbeddingInput) -> Result<Embedding> {
        let results = self.embed_batch(&[input.clone()]).await?;
        results.into_iter().next().ok_or_else(|| AppError::ExternalService {
            service: "NVIDIA".to_string(),
            message: "Failed to embed single input".to_string(),
        })
    }

    async fn embed_batch(&self, inputs: &[EmbeddingInput]) -> Result<Vec<Embedding>> {
        if self.api_key.is_empty() || self.api_key == "mock" {
            let mut results = Vec::new();
            for input in inputs {
                results.push(generate_mock_embedding(&input.text, 1024)?);
            }
            return Ok(results);
        }

        let mut all_embeddings = Vec::with_capacity(inputs.len());
        let batch_size = 50;

        for batch in inputs.chunks(batch_size) {
            let texts: Vec<&str> = batch.iter().map(|i| i.text.as_str()).collect();

            let retry_strategy = ExponentialBackoff::from_millis(100)
                .map(jitter)
                .take(3);

            let batch_embeddings = Retry::spawn(retry_strategy, || async {
                let response = self.client
                    .post("https://integrate.api.nvidia.com/v1/embeddings")
                    .bearer_auth(&self.api_key)
                    .json(&json!({
                        "input": texts.clone(),
                        "model": "nvidia/nv-embedqa-e5-v5",
                        "input_type": "query",
                        "encoding_format": "float"
                    }))
                    .send()
                    .await
                    .map_err(|e| AppError::ExternalService {
                        service: "NVIDIA".to_string(),
                        message: e.to_string(),
                    })?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(AppError::ExternalService {
                        service: "NVIDIA".to_string(),
                        message: format!("Status: {}, Body: {}", status, body),
                    });
                }

                let json_body: serde_json::Value = response.json().await.map_err(|e| AppError::ExternalService {
                    service: "NVIDIA".to_string(),
                    message: format!("Failed to parse response body as JSON: {}", e),
                })?;

                let embeddings: Vec<Embedding> = json_body["data"]
                    .as_array()
                    .ok_or_else(|| AppError::ExternalService {
                        service: "NVIDIA".to_string(),
                        message: "Missing 'data' field in NVIDIA response".to_string(),
                    })?
                    .iter()
                    .map(|item| {
                        let vector = item["embedding"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                            .collect();
                        Embedding {
                            vector,
                            dimensions: 1024,
                        }
                    })
                    .collect();

                Ok(embeddings)
            }).await?;

            all_embeddings.extend(batch_embeddings);
        }

        Ok(all_embeddings)
    }
}

/// Generates a deterministic mock vector based on string properties to simulate semantic search tests without API keys.
fn generate_mock_embedding(text: &str, dimensions: usize) -> Result<Embedding> {
    let mut vector = vec![0.0f32; dimensions];
    
    // Hash-based mock generation to make similar texts have slightly similar embeddings
    for (i, ch) in text.chars().enumerate() {
        let val = (ch as u32) as f32;
        let index = (i * 31 + val as usize) % dimensions;
        vector[index] += val.sin();
    }
    
    // Normalize vector
    let sum_sq: f32 = vector.iter().map(|x| x * x).sum();
    let norm = sum_sq.sqrt();
    if norm > 0.0 {
        for val in &mut vector {
            *val /= norm;
        }
    }

    Ok(Embedding {
        vector,
        dimensions,
    })
}
