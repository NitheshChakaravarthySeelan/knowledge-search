pub use sea_orm_migration::prelude::*;

mod m20240602_000001_create_document_jobs_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240602_000001_create_document_jobs_table::Migration),
        ]
    }
}
