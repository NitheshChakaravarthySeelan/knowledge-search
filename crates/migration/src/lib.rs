pub use sea_orm_migration::prelude::*;

mod m20240602_000001_create_document_jobs_table;
mod m20260605_000002_create_kb_graph_tables;
mod m20260607_000003_add_file_path_to_document_jobs;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20240602_000001_create_document_jobs_table::Migration),
            Box::new(m20260605_000002_create_kb_graph_tables::Migration),
            Box::new(m20260607_000003_add_file_path_to_document_jobs::Migration),
        ]
    }
}
