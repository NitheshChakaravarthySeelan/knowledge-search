use std::io::Error;

use text_splitter::{Characters, ChunkConfig, MarkdownSplitter};
use crate::models::document::Document;
use crate::models::document_chunk::DocumentChunk;
use crate::chunkers::Chunker;

pub struct RecursiveTextChunker {
    chunker: MarkdownSplitter<Characters>,
}

impl RecursiveTextChunker {
    pub fn new(chunk_size: usize, _chunk_overlap: usize) -> Self {
        Self {
            chunker: MarkdownSplitter::<_>::new(ChunkConfig::new(chunk_size)),
        }
    }

    pub fn default() -> Self {
        Self::new(1000, 200)
    }
}

impl Chunker for RecursiveTextChunker {
    fn chunk(&self, document: &Document) -> Result<Vec<DocumentChunk>, Error> {
        let chunks = self.chunker.chunk_indices(&document.content);
        let document_chunks: Vec<DocumentChunk> = chunks
            .enumerate()
            .map(|(i, (start_offset, chunk))| {
                let end_offset = start_offset + chunk.len();
                DocumentChunk {
                    id: uuid::Uuid::new_v4().to_string(),
                    document_id: document.id.clone(),
                    tenant_id: document.tenant_id.clone(),
                    content: document.content[start_offset..end_offset].to_string(),
                    index: i,
                    start_offset: start_offset,
                    end_offset: end_offset,
                    metadata: document.metadata.clone(),
                }
            })
            .collect();
        Ok(document_chunks)
    }
}
