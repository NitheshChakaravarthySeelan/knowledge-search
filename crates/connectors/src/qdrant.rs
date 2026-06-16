use anyhow::Result;
use qdrant_client::{
    qdrant::{
        Condition, CreateCollectionBuilder, DeletePointsBuilder, Distance, Filter, PointStruct,
        ScoredPoint, SearchPointsBuilder, UpsertPointsBuilder,
    },
    Payload, Qdrant,
};
use std::sync::Arc;
use documents::models::document_chunk::DocumentChunk;

pub struct QdrantClient {
    client: Arc<Qdrant>,
}

impl QdrantClient {
    pub fn new(url: &str) -> Result<Self> {
        let client = Qdrant::from_url(url).build()?;
        Ok(Self {
            client: Arc::new(client),
        })
    }

    pub async fn ensure_collection(&self, collection_name: &str, vector_size: u64) -> Result<()> {
        let collections = self.client.list_collections().await?;
        let exists = collections.collections.iter().any(|c| c.name == collection_name);
        
        if !exists {
            let mut sparse_map = std::collections::HashMap::new();
            sparse_map.insert(
                "sparse-text".to_string(),
                qdrant_client::qdrant::SparseVectorParams::default(),
            );
            let sparse_config = qdrant_client::qdrant::SparseVectorConfig {
                map: sparse_map,
            };

            self.client
                .create_collection(
                    CreateCollectionBuilder::new(collection_name.to_string())
                        .vectors_config(
                            qdrant_client::qdrant::VectorsConfig {
                                config: Some(qdrant_client::qdrant::vectors_config::Config::ParamsMap(
                                    qdrant_client::qdrant::VectorParamsMap {
                                        map: [
                                            ("dense-text".to_string(), qdrant_client::qdrant::VectorParams {
                                                size: vector_size,
                                                distance: Distance::Cosine as i32,
                                                ..Default::default()
                                            })
                                        ].into_iter().collect(),
                                    }
                                ))
                            }
                        )
                        .sparse_vectors_config(sparse_config)
                )
                .await?;
        }
        Ok(())
    }

    pub async fn upsert_chunks(
        &self,
        collection_name: &str,
        chunks: &[DocumentChunk],
        vectors: &[Vec<f32>],
    ) -> Result<()> {
        let mut points = Vec::new();

        for (chunk, vector) in chunks.iter().zip(vectors.iter()) {
            let payload: Payload = serde_json::json!({
                "document_id": chunk.document_id.0,
                "tenant_id": chunk.tenant_id.0,
                "content": chunk.content,
                "parent_content": chunk.parent_content,
                "index": chunk.index,
                "start_offset": chunk.start_offset,
                "end_offset": chunk.end_offset,
                "metadata": chunk.metadata,
            })
            .try_into()?;

            points.push(PointStruct::new(
                chunk.id.clone(),
                vector.clone(),
                payload,
            ));
        }

        self.client.upsert_points(
            UpsertPointsBuilder::new(collection_name.to_string(), points)
        ).await?;
        Ok(())
    }

    pub async fn upsert_chunks_hybrid(
        &self,
        collection_name: &str,
        chunks: &[DocumentChunk],
        dense_vectors: &[Vec<f32>],
        sparse_vectors: &[(Vec<u32>, Vec<f32>)],
    ) -> Result<()> {
        let mut points = Vec::new();

        for ((chunk, dense), (sparse_indices, sparse_values)) in chunks
            .iter()
            .zip(dense_vectors.iter())
            .zip(sparse_vectors.iter())
        {
            let entity_names: Vec<&str> = chunk.entities.iter().map(|e| e.name.as_str()).collect();

            let payload: Payload = serde_json::json!({
                "document_id": chunk.document_id.0,
                "tenant_id": chunk.tenant_id.0,
                "content": chunk.content,
                "parent_content": chunk.parent_content,
                "index": chunk.index,
                "start_offset": chunk.start_offset,
                "end_offset": chunk.end_offset,
                "metadata": chunk.metadata,
                "entities": chunk.entities,
                "entity_names": entity_names,
                "ingested_at": chunk.ingested_at,
            })
            .try_into()?;

            let mut vectors_map = std::collections::HashMap::new();
            
            vectors_map.insert("dense-text".to_string(), qdrant_client::qdrant::Vector::from(dense.clone()));
            
            let sparse = qdrant_client::qdrant::SparseVector {
                indices: sparse_indices.clone(),
                values: sparse_values.clone(),
            };
            vectors_map.insert("sparse-text".to_string(), qdrant_client::qdrant::Vector::from(sparse));

            let named_vectors = qdrant_client::qdrant::NamedVectors {
                vectors: vectors_map,
            };

            let vectors = qdrant_client::qdrant::Vectors::from(named_vectors);

            let mut point = PointStruct::new(
                chunk.id.clone(),
                vec![0.0f32],
                payload,
            );
            point.vectors = Some(vectors);

            points.push(point);
        }

        self.client.upsert_points(
            UpsertPointsBuilder::new(collection_name.to_string(), points)
        ).await?;
        Ok(())
    }

    pub async fn search(
        &self,
        collection_name: &str,
        query_vector: Vec<f32>,
        limit: u64,
        tenant_id: Option<&str>,
    ) -> Result<Vec<ScoredPoint>> {
        let mut search_builder = SearchPointsBuilder::new(collection_name.to_string(), query_vector, limit)
            .with_payload(true);

        if let Some(tenant) = tenant_id {
            search_builder = search_builder.filter(Filter::all([Condition::matches(
                "tenant_id",
                tenant.to_string(),
            )]));
        }

        let mut search_points = search_builder.build();
        search_points.vector_name = Some("dense-text".to_string());

        let response = self.client.search_points(search_points).await?;
        Ok(response.result)
    }

    pub async fn delete_points_by_document_id(
        &self,
        collection_name: &str,
        document_id: &str,
    ) -> Result<()> {
        let filter = Filter::must([Condition::matches("document_id", document_id.to_string())]);
        self.client
            .delete_points(
                DeletePointsBuilder::new(collection_name.to_string())
                    .points(filter)
                    .wait(true),
            )
            .await?;
        Ok(())
    }

    /// Search dense vector filtered by a set of document IDs.
    /// Used by GraphRetriever to find chunks belonging to graph-related documents.
    pub async fn search_by_document_ids(
        &self,
        collection_name: &str,
        query_vector: Vec<f32>,
        document_ids: &[String],
        limit: u64,
        tenant_id: Option<&str>,
    ) -> Result<Vec<ScoredPoint>> {
        if document_ids.is_empty() {
            return Ok(Vec::new());
        }

        let doc_ids: Vec<&str> = document_ids.iter().map(|s| s.as_str()).collect();
        let mut conditions: Vec<Condition> = doc_ids
            .iter()
            .map(|id| Condition::matches("document_id", id.to_string()))
            .collect();

        if let Some(tenant) = tenant_id {
            conditions.push(Condition::matches("tenant_id", tenant.to_string()));
        }

        let filter = Filter::must(conditions);

        let mut search_builder = SearchPointsBuilder::new(collection_name.to_string(), query_vector, limit)
            .with_payload(true)
            .filter(filter);

        let mut search_points = search_builder.build();
        search_points.vector_name = Some("dense-text".to_string());

        let response = self.client.search_points(search_points).await?;
        Ok(response.result)
    }

    pub async fn search_sparse(
        &self,
        collection_name: &str,
        sparse_indices: Vec<u32>,
        sparse_values: Vec<f32>,
        limit: u64,
        tenant_id: Option<&str>,
    ) -> Result<Vec<ScoredPoint>> {
        let sparse_idx = qdrant_client::qdrant::SparseIndices {
            data: sparse_indices,
        };

        let mut search_builder = SearchPointsBuilder::new(collection_name.to_string(), sparse_values, limit)
            .with_payload(true);

        if let Some(tenant) = tenant_id {
            search_builder = search_builder.filter(Filter::all([Condition::matches(
                "tenant_id",
                tenant.to_string(),
            )]));
        }

        let mut search_points = search_builder.build();
        search_points.sparse_indices = Some(sparse_idx);
        search_points.vector_name = Some("sparse-text".to_string());

        let response = self.client.search_points(search_points).await?;
        Ok(response.result)
    }
}
