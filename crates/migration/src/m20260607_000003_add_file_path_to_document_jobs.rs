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
                    .add_column(ColumnDef::new(DocumentJobs::FilePath).string().null())
                    .add_column(ColumnDef::new(DocumentJobs::ProgressStage).integer().default(0))
                    .add_column(ColumnDef::new(DocumentJobs::ProgressPercent).integer().default(0))
                    .add_column(ColumnDef::new(DocumentJobs::ProgressMessage).string().null())
                    .add_column(ColumnDef::new(DocumentJobs::ErrorMessage).string().null())
                    .add_column(ColumnDef::new(DocumentJobs::StartedAt).timestamp().null())
                    .add_column(ColumnDef::new(DocumentJobs::CompletedAt).timestamp().null())
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
                    .drop_column(DocumentJobs::ProgressStage)
                    .drop_column(DocumentJobs::ProgressPercent)
                    .drop_column(DocumentJobs::ProgressMessage)
                    .drop_column(DocumentJobs::ErrorMessage)
                    .drop_column(DocumentJobs::StartedAt)
                    .drop_column(DocumentJobs::CompletedAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum DocumentJobs {
    Table,
    FilePath,
    ProgressStage,
    ProgressPercent,
    ProgressMessage,
    ErrorMessage,
    StartedAt,
    CompletedAt,
}
