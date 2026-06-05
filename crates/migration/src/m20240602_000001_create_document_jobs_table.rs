use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DocumentJobs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DocumentJobs::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(DocumentJobs::TenantId).string().not_null())
                    .col(ColumnDef::new(DocumentJobs::Title).string().not_null())
                    .col(ColumnDef::new(DocumentJobs::Content).string().not_null())
                    .col(ColumnDef::new(DocumentJobs::FileExtension).string().null())
                    .col(
                        ColumnDef::new(DocumentJobs::Status)
                            .string()
                            .not_null()
                            .default("pending"),
                    )
                    .col(
                        ColumnDef::new(DocumentJobs::CreatedAt)
                            .timestamp_with_time_zone()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DocumentJobs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum DocumentJobs {
    Table,
    Id,
    TenantId,
    Title,
    Content,
    FileExtension,
    Status,
    CreatedAt,
}
