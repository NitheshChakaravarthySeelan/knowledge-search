use serde::{Deserialize, Serialize};
use common::types::{DocumentId, TenantId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: String,
    pub document_id: DocumentId,
    pub tenant_id: TenantId,
    pub content: String,
    pub index: usize,
    pub start_offset: usize,
    pub end_offset: usize,
    pub metadata: serde_json::Value,
} 