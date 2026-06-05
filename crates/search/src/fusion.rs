use std::collections::HashMap;
use crate::retrievers::SearchResult;

/// Reciprocal Rank Fusion (RRF) merges ranked result lists from multiple
/// retrieval algorithms into a single, unified ranking.
///
/// The RRF score for a document `d` across ranking lists `M` is:
///   RRF(d) = Σ  1 / (k + rank_m(d))
///
/// where `k` is a smoothing constant (default 60) that prevents
/// top-ranked items from dominating the fused score.
pub struct ReciprocalRankFusion {
    k: f32,
}

impl ReciprocalRankFusion {
    pub fn new(k: f32) -> Self {
        Self { k }
    }

    pub fn default() -> Self {
        Self::new(60.0)
    }

    /// Fuses multiple ranked lists of SearchResults into a single list,
    /// scored by RRF and sorted descending. Deduplicates by `chunk_id`.
    pub fn fuse(&self, ranked_lists: Vec<Vec<SearchResult>>) -> Vec<SearchResult> {
        // Accumulate RRF scores per chunk_id
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();
        // Keep the best version of each SearchResult (highest original score)
        let mut best_results: HashMap<String, SearchResult> = HashMap::new();

        for list in &ranked_lists {
            for (rank, result) in list.iter().enumerate() {
                let rrf_contribution = 1.0 / (self.k + (rank as f32 + 1.0));
                *rrf_scores.entry(result.chunk_id.clone()).or_insert(0.0) += rrf_contribution;

                best_results
                    .entry(result.chunk_id.clone())
                    .and_modify(|existing| {
                        // Keep the one with richer content (prefer parent_content if available)
                        if result.content.len() > existing.content.len() {
                            *existing = result.clone();
                        }
                    })
                    .or_insert_with(|| result.clone());
            }
        }

        // Build final results with RRF scores
        let mut fused: Vec<SearchResult> = best_results
            .into_iter()
            .map(|(chunk_id, mut result)| {
                result.score = *rrf_scores.get(&chunk_id).unwrap_or(&0.0);
                result
            })
            .collect();

        // Sort descending by fused RRF score
        fused.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        fused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dummy_result(id: &str, content: &str) -> SearchResult {
        SearchResult {
            chunk_id: id.to_string(),
            document_id: "doc-1".to_string(),
            content: content.to_string(),
            score: 1.0,
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn test_rrf_fusion() {
        let rrf = ReciprocalRankFusion::new(1.0); // Use small k for testing

        // List 1: A, B, C
        let list1 = vec![
            make_dummy_result("A", "content-A"),
            make_dummy_result("B", "content-B"),
            make_dummy_result("C", "content-C"),
        ];

        // List 2: C, A, D
        let list2 = vec![
            make_dummy_result("C", "content-C"),
            make_dummy_result("A", "content-A"),
            make_dummy_result("D", "content-D"),
        ];

        let fused = rrf.fuse(vec![list1, list2]);

        // Assert size is 4
        assert_eq!(fused.len(), 4);

        // Expected scores:
        // k = 1.0
        // A: rank 1 (1st list) + rank 2 (2nd list) => 1/(1+1) + 1/(1+2) = 1/2 + 1/3 = 0.5 + 0.333 = 0.8333
        // B: rank 2 (1st list) => 1/(1+2) = 1/3 = 0.3333
        // C: rank 3 (1st list) + rank 1 (2nd list) => 1/(1+3) + 1/(1+1) = 1/4 + 1/2 = 0.25 + 0.5 = 0.75
        // D: rank 3 (2nd list) => 1/(1+3) = 1/4 = 0.25
        //
        // So ordering should be: A, C, B, D
        assert_eq!(fused[0].chunk_id, "A");
        assert_eq!(fused[1].chunk_id, "C");
        assert_eq!(fused[2].chunk_id, "B");
        assert_eq!(fused[3].chunk_id, "D");
    }
}

