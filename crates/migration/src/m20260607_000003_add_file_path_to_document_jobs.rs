use sea_orm_migration::prelude::*;

/// Adds a `file_path` column to `document_jobs`.
/// This is the stable deduplication key used by the ingestion pipeline to detect
/// re-uploads of the same file. It is separate from `title` (human display name).
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(DocumentJobs::Table)
                    // Allow NULL so existing rows are not broken. Workers treat NULL as
                    // falling back to `title` for backwards compatibility.
                    .add_column(ColumnDef::new(DocumentJobs::FilePath).string().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(DocumentJobs::Table)
                    .drop_column(DocumentJobs::FilePath)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum DocumentJobs {
    Table,
    FilePath,
}
