use async_trait::async_trait;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use common::errors::Result;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

#[async_trait]
pub trait SparseEmbeddingProvider: Send + Sync {
    /// Generates a sparse vector representation for the given text.
    async fn embed_sparse(&self, text: &str) -> Result<SparseVector>;
}

/// A deterministic local sparse encoder that tokenizes text and hashes words 
/// to a large vocabulary space, applying basic TF (Term Frequency) weight calculation.
pub struct LocalHashingSparseEncoder {
    vocabulary_size: u32,
}

impl LocalHashingSparseEncoder {
    pub fn new(vocabulary_size: u32) -> Self {
        Self { vocabulary_size }
    }

    pub fn default() -> Self {
        // Default to a 100,000 vocabulary size limit
        Self::new(100_000)
    }
}

#[async_trait]
impl SparseEmbeddingProvider for LocalHashingSparseEncoder {
    async fn embed_sparse(&self, text: &str) -> Result<SparseVector> {
        let tokens: Vec<String> = text
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if tokens.is_empty() {
            return Ok(SparseVector {
                indices: vec![],
                values: vec![],
            });
        }

        // Count token frequencies mapped to vocabulary index hashes
        let mut index_weights: HashMap<u32, f32> = HashMap::new();
        let total_tokens = tokens.len() as f32;

        for token in tokens {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            token.hash(&mut &mut hasher);
            let raw_hash = hasher.finish();
            let index = (raw_hash % self.vocabulary_size as u64) as u32;

            let entry = index_weights.entry(index).or_insert(0.0);
            *entry += 1.0;
        }

        // Convert and sort by index (Qdrant REQUIRES sparse indices to be sorted in ascending order)
        let mut pairs: Vec<(u32, f32)> = index_weights
            .into_iter()
            .map(|(idx, freq)| {
                // Basic TF-IDF/BM25 approximation: term frequency normalized by document length
                let tf = freq / total_tokens;
                // Add log scaling to dampen high-frequency term domination
                let weight = (1.0 + tf).ln();
                (idx, weight)
            })
            .collect();

        pairs.sort_by_key(|&(idx, _)| idx);

        let (indices, values) = pairs.into_iter().unzip();

        Ok(SparseVector { indices, values })
    }
}
