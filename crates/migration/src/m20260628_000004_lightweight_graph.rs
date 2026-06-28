use sea_orm_migration::prelude::*;

/// Drops the heavyweight `content` and `parent_content` columns from `kb_nodes`.
///
/// These columns stored full source text that was never read by the GraphClient
/// (it only queries metadata: id, node_type, title, file_path). Content is served
/// from Qdrant payloads (chunks) and the document_jobs table (full documents).
///
/// The graph is now a lightweight traversal layer, like Obsidian's link graph.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(KbNodes::Table)
                    .drop_column(KbNodes::Content)
                    .drop_column(KbNodes::ParentContent)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(KbNodes::Table)
                    .add_column(ColumnDef::new(KbNodes::Content).string().not_null())
                    .add_column(ColumnDef::new(KbNodes::ParentContent).string())
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum KbNodes {
    Table,
    Content,
    ParentContent,
}
