use common::types::{DocumentId, TenantId};
use std::io::Error;
use crate::models::document::Document;


pub trait Loader: Send + Sync {
    fn load(&self, id: DocumentId, tenant_id: TenantId, input: &[u8], title: String) -> Result<Document, Error>;
}