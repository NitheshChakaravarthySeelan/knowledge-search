use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. Create kb_nodes table
        manager
            .create_table(
                Table::create()
                    .table(KbNodes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(KbNodes::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(KbNodes::ParentId).uuid().null())
                    .col(ColumnDef::new(KbNodes::TenantId).string().not_null())
                    .col(ColumnDef::new(KbNodes::SourceType).string().not_null())
                    .col(ColumnDef::new(KbNodes::FilePath).string().not_null())
                    .col(ColumnDef::new(KbNodes::NodeType).string().not_null())
                    .col(ColumnDef::new(KbNodes::Title).string().null())
                    .col(ColumnDef::new(KbNodes::Content).string().not_null())
                    .col(ColumnDef::new(KbNodes::ParentContent).string().null())
                    .col(ColumnDef::new(KbNodes::ContentHash).string().not_null())
                    .col(ColumnDef::new(KbNodes::Metadata).json_binary().null())
                    .col(
                        ColumnDef::new(KbNodes::CreatedAt)
                            .timestamp_with_time_zone()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-kbnodes-parent_id")
                            .from(KbNodes::Table, KbNodes::ParentId)
                            .to(KbNodes::Table, KbNodes::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes for kb_nodes
        manager
            .create_index(
                Index::create()
                    .name("idx-kbnodes-tenant-filepath")
                    .table(KbNodes::Table)
                    .col(KbNodes::TenantId)
                    .col(KbNodes::FilePath)
                    .to_owned(),
            )
            .await?;

        // 2. Create kb_graph_edges table
        manager
            .create_table(
                Table::create()
                    .table(KbGraphEdges::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(KbGraphEdges::SourceId).uuid().not_null())
                    .col(ColumnDef::new(KbGraphEdges::TargetId).uuid().not_null())
                    .col(ColumnDef::new(KbGraphEdges::RelationType).string().not_null())
                    .col(ColumnDef::new(KbGraphEdges::TenantId).string().not_null())
                    .col(ColumnDef::new(KbGraphEdges::Metadata).json_binary().null())
                    .primary_key(
                        Index::create()
                            .col(KbGraphEdges::SourceId)
                            .col(KbGraphEdges::TargetId)
                            .col(KbGraphEdges::RelationType),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-kbgraph-source_id")
                            .from(KbGraphEdges::Table, KbGraphEdges::SourceId)
                            .to(KbNodes::Table, KbNodes::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-kbgraph-target_id")
                            .from(KbGraphEdges::Table, KbGraphEdges::TargetId)
                            .to(KbNodes::Table, KbNodes::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create indexes for kb_graph_edges
        manager
            .create_index(
                Index::create()
                    .name("idx-kbgraph-source")
                    .table(KbGraphEdges::Table)
                    .col(KbGraphEdges::TenantId)
                    .col(KbGraphEdges::SourceId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-kbgraph-target")
                    .table(KbGraphEdges::Table)
                    .col(KbGraphEdges::TenantId)
                    .col(KbGraphEdges::TargetId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(KbGraphEdges::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(KbNodes::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum KbNodes {
    Table,
    Id,
    ParentId,
    TenantId,
    SourceType,
    FilePath,
    NodeType,
    Title,
    Content,
    ParentContent,
    ContentHash,
    Metadata,
    CreatedAt,
}

#[derive(DeriveIden)]
enum KbGraphEdges {
    Table,
    SourceId,
    TargetId,
    RelationType,
    TenantId,
    Metadata,
}
