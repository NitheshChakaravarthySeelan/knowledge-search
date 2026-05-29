use std::sync::Arc;
use common::errors::Result;
use common::types::TenantId;
use crate::retrievers::{Retriever, SearchResult};
use crate::rerankers::Reranker;

pub struct SearchService {
    retriever: Arc<dyn Retriever>,
    reranker: Arc<dyn Reranker>,
}

impl SearchService {
    pub fn new(retriever: Arc<dyn Retriever>, reranker: Arc<dyn Reranker>) -> Self {
        Self { retriever, reranker }
    }

    /// Performs a high-performance, multi-stage hybrid search and reranking process.
    pub async fn search(&self, tenant_id: &TenantId, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Stage 1: Retrieve candidates
        let candidates = self.retriever.retrieve(tenant_id, query, limit * 2).await?;
        
        // Stage 2: Rerank candidates
        let reranked = self.reranker.rerank(query, candidates).await?;
        
        // Take requested limit
        let mut final_results = reranked;
        final_results.truncate(limit);
        
        Ok(final_results)
    }
}
