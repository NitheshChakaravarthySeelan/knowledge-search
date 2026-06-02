use std::io::Error;

use common::types::{DocumentId, SourceType, TenantId};

use crate::models::document::Document;
use crate::loaders::Loader;

pub struct FileLoader {}

impl FileLoader {
    pub fn new() -> Self {
        Self {}
    }
}

impl Loader for FileLoader {
    fn load(
        &self,
        id: DocumentId,
        tenant_id: TenantId,
        input: &[u8],
        title: String,
    ) -> Result<Document, Error> {  
        let content = std::str::from_utf8(input)
            .map_err(|e| Error::new(std::io::ErrorKind::InvalidData, e))?
            .to_string();

        Ok(Document {
            id,
            tenant_id,
            source_type: SourceType::FileUpload,
            title,
            content,
            metadata: serde_json::json!({
                "loader": "FileLoader",
            }),
            version: 1,
        })
    }
}