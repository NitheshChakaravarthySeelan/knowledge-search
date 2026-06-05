use async_trait::async_trait;
use common::errors::{AppError, Result};
use common::types::TenantId;
use embeddings::traits::EmbeddingProvider;
use embeddings::models::EmbeddingInput;
use embeddings::sparse::SparseEmbeddingProvider;
use connectors::QdrantClient;
use std::sync::Arc;

use crate::retrievers::{Retriever, SearchResult};
use crate::fusion::ReciprocalRankFusion;

/// HybridRetriever performs both dense (semantic) and sparse (lexical) search
/// against Qdrant, then fuses the two ranked lists using Reciprocal Rank Fusion.
///
/// After fusion, it expands matched child chunks to their parent context,
/// deduplicating parents so the LLM receives complete, non-redundant paragraphs.
pub struct HybridRetriever {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    sparse_provider: Arc<dyn SparseEmbeddingProvider>,
    qdrant_client: Arc<QdrantClient>,
    collection_name: String,
    rrf: ReciprocalRankFusion,
}

impl HybridRetriever {
    pub fn new(
        embedding_provider: Arc<dyn EmbeddingProvider>,
        sparse_provider: Arc<dyn SparseEmbeddingProvider>,
        qdrant_client: Arc<QdrantClient>,
        collection_name: String,
    ) -> Self {
        Self {
            embedding_provider,
            sparse_provider,
            qdrant_client,
            collection_name,
            rrf: ReciprocalRankFusion::default(),
        }
    }
}

/// Extracts a chunk_id string from a Qdrant point ID.
fn extract_chunk_id(point_id: Option<qdrant_client::qdrant::PointId>) -> String {
    match point_id {
        Some(id) => match id.point_id_options {
            Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(n)) => n.to_string(),
            Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(s)) => s,
            None => "unknown".to_string(),
        },
        None => "unknown".to_string(),
    }
}

/// Maps a Qdrant ScoredPoint into a SearchResult, preferring parent_content
/// over child content when available (hierarchical context expansion).
fn scored_point_to_result(point: qdrant_client::qdrant::ScoredPoint) -> SearchResult {
    let metadata = serde_json::json!(point.payload);
    let chunk_id = extract_chunk_id(point.id);

    // Context expansion: prefer parent_content if it exists
    let content = metadata
        .get("parent_content")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| metadata.get("content").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    SearchResult {
        chunk_id,
        document_id: metadata
            .get("document_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        content,
        score: point.score,
        metadata,
    }
}

#[async_trait]
impl Retriever for HybridRetriever {
    async fn retrieve(
        &self,
        tenant_id: &TenantId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        // Prefetch factor: retrieve more candidates than final limit for better fusion
        let prefetch_limit = (limit * 3).max(10) as u64;

        // --- Stage 1: Dense search (semantic) ---
        let dense_input = EmbeddingInput {
            text: query.to_string(),
            user_id: None,
        };
        let dense_embedding = self.embedding_provider.embed(&dense_input).await?;

        let dense_results = self
            .qdrant_client
            .search(
                &self.collection_name,
                dense_embedding.vector,
                prefetch_limit,
                Some(&tenant_id.0),
            )
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Qdrant-Dense".to_string(),
                message: e.to_string(),
            })?;

        let dense_search_results: Vec<SearchResult> = dense_results
            .into_iter()
            .map(scored_point_to_result)
            .collect();

        tracing::debug!(
            tenant = tenant_id.0,
            query = query,
            dense_hits = dense_search_results.len(),
            "Dense search completed"
        );

        // --- Stage 2: Sparse search (lexical) ---
        let sparse_embedding = self.sparse_provider.embed_sparse(query).await?;

        let sparse_results = self
            .qdrant_client
            .search_sparse(
                &self.collection_name,
                sparse_embedding.indices,
                sparse_embedding.values,
                prefetch_limit,
                Some(&tenant_id.0),
            )
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Qdrant-Sparse".to_string(),
                message: e.to_string(),
            })?;

        let sparse_search_results: Vec<SearchResult> = sparse_results
            .into_iter()
            .map(scored_point_to_result)
            .collect();

        tracing::debug!(
            tenant = tenant_id.0,
            query = query,
            sparse_hits = sparse_search_results.len(),
            "Sparse search completed"
        );

        // --- Stage 3: Reciprocal Rank Fusion ---
        let mut fused = self
            .rrf
            .fuse(vec![dense_search_results, sparse_search_results]);

        // Deduplicate by parent content to avoid sending the same
        // expanded paragraph multiple times to the LLM
        let mut seen_parents = std::collections::HashSet::new();
        fused.retain(|r| {
            // Use a hash of the first 200 chars of content as dedup key
            let key = r.content.chars().take(200).collect::<String>();
            seen_parents.insert(key)
        });

        fused.truncate(limit);

        tracing::info!(
            tenant = tenant_id.0,
            query = query,
            final_results = fused.len(),
            "Hybrid RRF fusion completed"
        );

        Ok(fused)
    }
}
