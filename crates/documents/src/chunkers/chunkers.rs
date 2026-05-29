use std::io::Error;

use crate::models::{
    document::Document,
    document_chunk::DocumentChunk,
};

pub trait Chunker: Send + Sync {
    fn chunk(&self, document: &Document) -> Result<Vec<DocumentChunk>, Error>;
}
