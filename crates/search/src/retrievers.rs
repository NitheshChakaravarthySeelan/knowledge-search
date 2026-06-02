use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use common::errors::{AppError, Result};
use common::types::TenantId;
use embeddings::traits::EmbeddingProvider;
use embeddings::models::EmbeddingInput;
use connectors::QdrantClient;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub document_id: String,
    pub content: String,
    pub score: f32,
    pub metadata: serde_json::Value,
}

#[async_trait]
pub trait Retriever: Send + Sync {
    /// Performs a search operation using a natural language query string.
    async fn retrieve(&self, tenant_id: &TenantId, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
}

pub struct VectorRetriever {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    qdrant_client: Arc<QdrantClient>,
    collection_name: String,
}

impl VectorRetriever {
    pub fn new(
        embedding_provider: Arc<dyn EmbeddingProvider>,
        qdrant_client: Arc<QdrantClient>,
        collection_name: String,
    ) -> Self {
        Self {
            embedding_provider,
            qdrant_client,
            collection_name,
        }
    }
}

#[async_trait]
impl Retriever for VectorRetriever {
    async fn retrieve(&self, tenant_id: &TenantId, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // 1. Generate the query vector
        let input = EmbeddingInput {
            text: query.to_string(),
            user_id: None,
        };
        let embedding = self.embedding_provider.embed(&input).await?;
        
        tracing::debug!(
            tenant = tenant_id.0,
            query = query,
            collection = self.collection_name,
            "Executing vector search query..."
        );

        // 2. Query Qdrant
        let results = self.qdrant_client
            .search(
                &self.collection_name,
                embedding.vector,
                limit as u64,
                Some(&tenant_id.0),
            )
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Qdrant".to_string(),
                message: e.to_string(),
            })?;

        // 3. Map Qdrant points to SearchResult
        let search_results = results.into_iter().map(|point| {
            let metadata = serde_json::json!(point.payload);
            
            let chunk_id = match point.id {
                Some(id) => match id.point_id_options {
                    Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(n)) => n.to_string(),
                    Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(s)) => s,
                    None => "unknown".to_string(),
                },
                None => "unknown".to_string(),
            };

            SearchResult {
                chunk_id,
                document_id: metadata.get("document_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                content: metadata.get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                score: point.score,
                metadata,
            }
        }).collect();

        Ok(search_results)
    }
}
