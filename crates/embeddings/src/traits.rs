use async_trait::async_trait;
use common::errors::Result;
use crate::models::{Embedding, EmbeddingInput};

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generates a vector embedding for a single text input.
    async fn embed(&self, input: &EmbeddingInput) -> Result<Embedding>;

    /// Generates vector embeddings for a list of text inputs in parallel or batch.
    async fn embed_batch(&self, inputs: &[EmbeddingInput]) -> Result<Vec<Embedding>>;
}
