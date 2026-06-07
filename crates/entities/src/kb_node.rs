use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "kb_nodes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: uuid::Uuid,
    pub parent_id: Option<uuid::Uuid>,
    pub tenant_id: String,
    pub source_type: String,
    pub file_path: String,
    pub node_type: String,
    pub title: Option<String>,
    pub content: String,
    pub parent_content: Option<String>,
    pub content_hash: String,
    pub metadata: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentId",
        to = "Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    SelfRef,
}

impl ActiveModelBehavior for ActiveModel {}
