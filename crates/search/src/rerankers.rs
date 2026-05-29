use async_trait::async_trait;
use common::errors::Result;
use crate::retrievers::SearchResult;

#[async_trait]
pub trait Reranker: Send + Sync {
    /// Re-evaluates and ranks a list of search results against the original query.
    async fn rerank(&self, query: &str, results: Vec<SearchResult>) -> Result<Vec<SearchResult>>;
}

pub struct LocalReranker;

#[async_trait]
impl Reranker for LocalReranker {
    async fn rerank(&self, query: &str, mut results: Vec<SearchResult>) -> Result<Vec<SearchResult>> {
        let query_grams = get_bi_grams(&query.to_lowercase());

        for result in &mut results {
            let doc_grams = get_bi_grams(&result.content.to_lowercase());
            
            // Jaccard similarity score
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

            // Interpolate original vector search score with Jaccard overlap score
            result.score = (result.score * 0.7) + (similarity * 0.3);
        }

        // Sort by adjusted score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results)
    }
}

/// Tokenizes text into character bi-grams (two-letter substrings) for text overlap analysis.
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
