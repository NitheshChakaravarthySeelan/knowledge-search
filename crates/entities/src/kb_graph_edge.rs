use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "kb_graph_edges")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub source_id: uuid::Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub target_id: uuid::Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub relation_type: String,
    pub tenant_id: String,
    pub metadata: Option<Json>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::kb_node::Entity",
        from = "Column::SourceId",
        to = "super::kb_node::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    SourceNode,
    #[sea_orm(
        belongs_to = "super::kb_node::Entity",
        from = "Column::TargetId",
        to = "super::kb_node::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    TargetNode,
}

impl ActiveModelBehavior for ActiveModel {}
