use std::io::Error;

use common::types::{DocumentId, SourceType, TenantId};

use crate::models::document::Document;
use crate::loaders::Loader;

pub struct FileLoader {
    file_path: String,
}

impl FileLoader {
    pub fn new(file_path: String) -> Self {
        Self {
            file_path,
        }
    }
}

impl Loader for FileLoader {
    fn load(&self, id: DocumentId, tenant_id: TenantId, input: &[u8], title: String) -> Result<Document, Error> {
        let content = std::fs::read_to_string(&self.file_path)?;
        Ok(Document {
            id,
            tenant_id,
            source_type: SourceType::FileUpload,
            title,
            content,
            metadata: serde_json::json!({
                "file_path": self.file_path,
            }),
            version: 1,
        })
    }
}