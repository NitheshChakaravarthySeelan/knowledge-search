use async_trait::async_trait;
use common::errors::{AppError, Result};
use crate::retrievers::SearchResult;
use reqwest::Client;
use serde_json::json;

#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(&self, query: &str, results: Vec<SearchResult>) -> Result<Vec<SearchResult>>;
}

pub struct CohereReranker {
    api_key: String,
    client: Client,
}

impl CohereReranker {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Reranker for CohereReranker {
    async fn rerank(&self, query: &str, mut results: Vec<SearchResult>) -> Result<Vec<SearchResult>> {
        if results.is_empty() {
            return Ok(results);
        }

        let documents: Vec<String> = results.iter().map(|r| r.content.clone()).collect();

        let response = self.client
            .post("https://api.cohere.ai/v1/rerank")
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": "rerank-english-v3.0",
                "query": query,
                "documents": documents,
                "top_n": results.len()
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Cohere-Rerank".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(AppError::ExternalService {
                service: "Cohere-Rerank".to_string(),
                message: format!("Status: {}", response.status()),
            });
        }

        let json_body: serde_json::Value = response.json().await
            .map_err(|e| AppError::ExternalService {
                service: "Cohere-Rerank".to_string(),
                message: e.to_string(),
            })?;

        let reranked_data = json_body["results"]
            .as_array()
            .ok_or_else(|| AppError::ExternalService {
                service: "Cohere-Rerank".to_string(),
                message: "Missing 'results' in Cohere response".to_string(),
            })?;

        for item in reranked_data {
            let index = item["index"].as_u64().ok_or_else(|| AppError::ExternalService {
                service: "Cohere-Rerank".to_string(),
                message: "Missing 'index' in Cohere result".to_string(),
            })? as usize;
            
            let score = item["relevance_score"].as_f64().unwrap_or(0.0) as f32;
            results[index].score = score;
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results)
    }
}

// Keep LocalReranker for fallback
pub struct LocalReranker;

#[async_trait]
impl Reranker for LocalReranker {
    async fn rerank(&self, query: &str, mut results: Vec<SearchResult>) -> Result<Vec<SearchResult>> {
        let query_grams = get_bi_grams(&query.to_lowercase());

        for result in &mut results {
            let doc_grams = get_bi_grams(&result.content.to_lowercase());
            
            let intersection: usize = query_grams
                .iter()
                .filter(|g| doc_grams.contains(*g))
                .count();
                
            let union = query_grams.len() + doc_grams.len() - intersection;
            
            let similarity = if union > 0 {
                intersection as f32 / union as f32
            } else {
                0.0
            };

            result.score = (result.score * 0.7) + (similarity * 0.3);
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results)
    }
}

fn get_bi_grams(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
    if chars.len() < 2 {
        return vec![text.to_string()];
    }

    let mut grams = Vec::new();
    for i in 0..chars.len() - 1 {
        let gram: String = chars[i..i + 2].iter().collect();
        grams.push(gram);
    }
    grams
}
