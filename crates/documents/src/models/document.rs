use serde::{Deserialize, Serialize};
use common::types::{DocumentId, TenantId, SourceType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub tenant_id: TenantId,
    pub source_type: SourceType,
    pub title: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub version: i32,
}