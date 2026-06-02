use anyhow::Result;
use qdrant_client::{
    qdrant::{
        CreateCollectionBuilder, Distance, VectorParamsBuilder, 
        Filter, Condition, ScoredPoint, SearchPointsBuilder, UpsertPointsBuilder, PointStruct,
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
            self.client
                .create_collection(
                    CreateCollectionBuilder::new(collection_name.to_string())
                        .vectors_config(VectorParamsBuilder::new(vector_size, Distance::Cosine))
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

        let response = self.client.search_points(search_builder).await?;
        Ok(response.result)
    }
}
