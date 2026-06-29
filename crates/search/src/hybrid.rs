use async_trait::async_trait;
use common::errors::{AppError, Result};
use common::types::TenantId;
use embeddings::traits::EmbeddingProvider;
use embeddings::models::EmbeddingInput;
use embeddings::sparse::SparseEmbeddingProvider;
use connectors::QdrantClient;
use documents::EntityExtractor;
use std::sync::Arc;

use crate::graph_retriever::GraphRetriever;
use crate::query_transform::QueryTransformer;
use crate::retrievers::{Retriever, SearchConfig, SearchResult};
use crate::fusion::ReciprocalRankFusion;

/// HybridRetriever performs dense (semantic), sparse (lexical), entity-boosted,
/// and graph-traversal search in parallel, then fuses all ranked lists using
/// weighted Reciprocal Rank Fusion.
pub struct HybridRetriever {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    sparse_provider: Arc<dyn SparseEmbeddingProvider>,
    qdrant_client: Arc<QdrantClient>,
    collection_name: String,
    graph_retriever: Option<Arc<GraphRetriever>>,
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
            graph_retriever: None,
        }
    }

    pub fn with_graph_retriever(
        embedding_provider: Arc<dyn EmbeddingProvider>,
        sparse_provider: Arc<dyn SparseEmbeddingProvider>,
        qdrant_client: Arc<QdrantClient>,
        collection_name: String,
        graph_retriever: Arc<GraphRetriever>,
    ) -> Self {
        Self {
            embedding_provider,
            sparse_provider,
            qdrant_client,
            collection_name,
            graph_retriever: Some(graph_retriever),
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
pub fn scored_point_to_result(point: qdrant_client::qdrant::ScoredPoint) -> SearchResult {
    let metadata = serde_json::json!(point.payload);
    let chunk_id = extract_chunk_id(point.id);

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
    async fn retrieve_with_config(
        &self,
        tenant_id: &TenantId,
        query: &str,
        limit: usize,
        config: &SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        let prefetch_limit = (limit * 3).max(10) as u64;

        // --- Transform query: expand acronyms, generate variants ---
        let tq = QueryTransformer::transform(query);
        let search_query = &tq.primary;
        tracing::debug!(
            original = query,
            transformed = search_query,
            variants = tq.variants.len(),
            "Query transformation applied"
        );

        // --- Extract entities once (shared by entity + graph passes) ---
        // Entity extraction uses the ORIGINAL query (entities are user-facing terms,
        // not acronym-expanded forms).
        let (entity_names, _) = EntityExtractor::extract_for_content(
            "query",
            query,
            None,
        );
        let entity_names_str: String = entity_names.iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let has_entities = !entity_names_str.is_empty();

        // --- Launch all 4 main searches in parallel ---
        // Dense and sparse use the TRANSFORMED query (acronyms expanded).
        // Entity and graph use the ORIGINAL query (entities from user's words).
        let dense_fut = async {
            let input = EmbeddingInput {
                text: search_query.to_string(),
                user_id: None,
            };
            let embedding = self.embedding_provider.embed(&input).await?;
            let results = self
                .qdrant_client
                .search(
                    &self.collection_name,
                    embedding.vector,
                    prefetch_limit,
                    Some(&tenant_id.0),
                )
                .await
                .map_err(|e| AppError::ExternalService {
                    service: "Qdrant-Dense".to_string(),
                    message: e.to_string(),
                })?;
            let r: Vec<SearchResult> = results.into_iter().map(scored_point_to_result).collect();
            tracing::debug!(hits = r.len(), "Dense search done");
            Ok::<_, AppError>(r)
        };

        let sparse_fut = async {
            let sparse_embedding = self.sparse_provider.embed_sparse(search_query).await?;
            let results = self
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
            let r: Vec<SearchResult> = results.into_iter().map(scored_point_to_result).collect();
            tracing::debug!(hits = r.len(), "Sparse search done");
            Ok::<_, AppError>(r)
        };

        let entity_query = entity_names_str.clone();
        let entity_fut = async {
            if !has_entities {
                return Ok(Vec::new());
            }
            let input = EmbeddingInput {
                text: entity_query,
                user_id: None,
            };
            let embedding = self.embedding_provider.embed(&input).await
                .map_err(|e| AppError::ExternalService {
                    service: "Entity-Embedding".to_string(),
                    message: e.to_string(),
                })?;
            let results = self
                .qdrant_client
                .search(
                    &self.collection_name,
                    embedding.vector,
                    prefetch_limit,
                    Some(&tenant_id.0),
                )
                .await
                .map_err(|e| AppError::ExternalService {
                    service: "Qdrant-Entity".to_string(),
                    message: e.to_string(),
                })?;
            let r: Vec<SearchResult> = results.into_iter().map(scored_point_to_result).collect();
            tracing::debug!(hits = r.len(), "Entity search done");
            Ok::<_, AppError>(r)
        };

        let graph_fut = async {
            match &self.graph_retriever {
                Some(gr) if has_entities => {
                    gr.retrieve(tenant_id, query, prefetch_limit).await
                }
                _ => Ok(Vec::new()),
            }
        };

        let (dense_res, sparse_res, entity_res, graph_res) = tokio::join!(
            dense_fut,
            sparse_fut,
            entity_fut,
            graph_fut,
        );

        let dense_search_results = dense_res.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Dense search failed, skipping");
            Vec::new()
        });
        let sparse_search_results = sparse_res.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Sparse search failed, skipping");
            Vec::new()
        });
        let entity_search_results = entity_res.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Entity search failed, skipping");
            Vec::new()
        });
        let graph_search_results = graph_res.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Graph search failed, skipping");
            Vec::new()
        });

        // --- Weighted RRF Fusion (dense + sparse + entity + graph) ---
        let mut all_lists: Vec<Vec<SearchResult>> = Vec::new();
        let mut all_weights: Vec<f32> = Vec::new();

        if !dense_search_results.is_empty() {
            all_lists.push(dense_search_results);
            all_weights.push(config.dense_weight);
        }
        if !sparse_search_results.is_empty() {
            all_lists.push(sparse_search_results);
            all_weights.push(config.sparse_weight);
        }
        if !entity_search_results.is_empty() {
            all_lists.push(entity_search_results);
            all_weights.push(config.entity_weight);
        }
        if !graph_search_results.is_empty() {
            all_lists.push(graph_search_results);
            all_weights.push(config.graph_weight);
        }

        let rrf = ReciprocalRankFusion::new(config.rrf_k, all_weights);
        let mut fused = rrf.fuse(all_lists);

        // Deduplicate by parent content
        let mut seen_parents = std::collections::HashSet::new();
        fused.retain(|r| {
            let key = r.content.chars().take(200).collect::<String>();
            seen_parents.insert(key)
        });

        fused.truncate(limit);

        // --- Variant search: for compound questions, search each sub-query ---
        // These add precision for multi-part questions like "What is X? How does Y work?"
        if !tq.variants.is_empty() {
            for variant in &tq.variants {
                let input = EmbeddingInput {
                    text: variant.clone(),
                    user_id: None,
                };
                match self.embedding_provider.embed(&input).await {
                    Ok(embedding) => {
                        match self
                            .qdrant_client
                            .search(
                                &self.collection_name,
                                embedding.vector,
                                prefetch_limit,
                                Some(&tenant_id.0),
                            )
                            .await
                        {
                            Ok(points) => {
                                let results: Vec<SearchResult> = points
                                    .into_iter()
                                    .map(scored_point_to_result)
                                    .collect();
                                if !results.is_empty() {
                                    // Lower weight for variants — they're auxiliary
                                    let variant_rrf =
                                        ReciprocalRankFusion::new(config.rrf_k, vec![0.5]);
                                    fused = variant_rrf.fuse(vec![fused, results]);
                                    fused.truncate(limit);
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    variant = variant,
                                    error = %e,
                                    "Variant search failed, skipping"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            variant = variant,
                            error = %e,
                            "Variant embedding failed, skipping"
                        );
                    }
                }
            }
        }

        tracing::info!(
            tenant = tenant_id.0,
            query = query,
            transformed = search_query,
            final_results = fused.len(),
            "Hybrid RRF fusion completed (4-pass parallel + variants)"
        );

        Ok(fused)
    }
}
