use std::sync::Arc;

use common::errors::{AppError, Result};
use common::types::TenantId;
use connectors::{GraphClient, QdrantClient};
use documents::{EntityExtractor, ExtractedEntity};
use embeddings::models::EmbeddingInput;
use embeddings::traits::EmbeddingProvider;

use crate::retrievers::SearchResult;
use crate::hybrid::scored_point_to_result;



/// Retrieves chunks from documents related to query entities via the
/// knowledge graph. Extracts entities from the query, looks them up
/// in kb_nodes, traverses kb_graph_edges to find related documents,
/// then searches Qdrant filtered by those document IDs.
pub struct GraphRetriever {
    graph_client: Arc<GraphClient>,
    qdrant_client: Arc<QdrantClient>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    collection_name: String,
}

impl GraphRetriever {
    pub fn new(
        graph_client: Arc<GraphClient>,
        qdrant_client: Arc<QdrantClient>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        collection_name: String,
    ) -> Self {
        Self {
            graph_client,
            qdrant_client,
            embedding_provider,
            collection_name,
        }
    }

    pub async fn retrieve(
        &self,
        tenant_id: &TenantId,
        query: &str,
        limit: u64,
    ) -> Result<Vec<SearchResult>> {
        // 1. Extract entity names from query
        let (entities, _): (Vec<ExtractedEntity>, _) =
            EntityExtractor::extract_for_content("query", query, None);
        let entity_names: Vec<String> = entities.into_iter().map(|e| e.name).collect();

        if entity_names.is_empty() {
            return Ok(Vec::new());
        }

        tracing::debug!(
            tenant = tenant_id.0,
            entity_count = entity_names.len(),
            "GraphRetriever: extracted entities from query"
        );

        // 2. Look up matching Document nodes in the graph
        let nodes = self
            .graph_client
            .lookup_nodes_by_entity_name(&entity_names, &tenant_id.0)
            .await
            .map_err(|e| AppError::Database(format!("Graph node lookup failed: {}", e)))?;

        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        let node_ids: Vec<uuid::Uuid> = nodes.iter().map(|n| n.id).collect();

        tracing::debug!(
            node_count = node_ids.len(),
            "GraphRetriever: found matching graph nodes"
        );

        // 3. Traverse edges (1-2 hops) to find related nodes
        let edges = self
            .graph_client
            .traverse_edges(&node_ids, &tenant_id.0, 2)
            .await
            .map_err(|e| AppError::Database(format!("Graph edge traversal failed: {}", e)))?;

        // Collect all connected node IDs
        let mut connected_ids: Vec<uuid::Uuid> = edges
            .iter()
            .flat_map(|e| vec![e.source_id, e.target_id])
            .collect();
        connected_ids.extend(node_ids);
        connected_ids.sort();
        connected_ids.dedup();

        // 4. Resolve to Document node UUIDs
        let document_uuids = self
            .graph_client
            .resolve_document_ids(&connected_ids, &tenant_id.0)
            .await
            .map_err(|e| AppError::Database(format!("Document ID resolution failed: {}", e)))?;

        if document_uuids.is_empty() {
            return Ok(Vec::new());
        }

        tracing::debug!(
            doc_count = document_uuids.len(),
            "GraphRetriever: resolved to document IDs"
        );

        // 5. Embed the original query for scoring
        let input = EmbeddingInput {
            text: query.to_string(),
            user_id: None,
        };
        let embedding = self
            .embedding_provider
            .embed(&input)
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Graph-Embedding".to_string(),
                message: e.to_string(),
            })?;

        // 6. Search Qdrant filtered by document IDs
        let results = self
            .qdrant_client
            .search_by_document_ids(
                &self.collection_name,
                embedding.vector,
                &document_uuids,
                limit,
                Some(&tenant_id.0),
            )
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Qdrant-Graph".to_string(),
                message: e.to_string(),
            })?;

        let search_results: Vec<SearchResult> = results
            .into_iter()
            .map(scored_point_to_result)
            .collect();

        tracing::info!(
            tenant = tenant_id.0,
            query = query,
            graph_hits = search_results.len(),
            "Graph search completed"
        );

        Ok(search_results)
    }
}
