use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "document_jobs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: uuid::Uuid,
    pub tenant_id: String,
    pub title: String,
    /// Stable deduplication key (e.g. relative file path, S3 key, or connector-specific URI).
    /// Added in migration 3. Falls back to `title` if NULL for legacy rows.
    pub file_path: Option<String>,
    pub content: String,
    pub file_extension: Option<String>,
    pub status: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
