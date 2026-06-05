use std::io::Error;
use text_splitter::{Characters, ChunkConfig, MarkdownSplitter};

use crate::models::document::Document;
use crate::models::document_chunk::DocumentChunk;
use crate::chunkers::Chunker;

pub struct HierarchicalChunker {
    parent_splitter: MarkdownSplitter<Characters>,
    child_splitter: MarkdownSplitter<Characters>,
}

impl HierarchicalChunker {
    pub fn new(
        parent_chunk_size: usize,
        child_chunk_size: usize,
    ) -> Self {
        Self {
            parent_splitter: MarkdownSplitter::<_>::new(ChunkConfig::new(parent_chunk_size)),
            child_splitter: MarkdownSplitter::<_>::new(ChunkConfig::new(child_chunk_size)),
        }
    }

    pub fn default() -> Self {
        Self::new(1500, 300)
    }
}

impl Chunker for HierarchicalChunker {
    fn chunk(&self, document: &Document) -> Result<Vec<DocumentChunk>, Error> {
        let parent_chunks = self.parent_splitter.chunk_indices(&document.content);
        let mut document_chunks = Vec::new();
        let mut global_index = 0;

        for (parent_start_offset, parent_text) in parent_chunks {
            let child_chunks = self.child_splitter.chunk_indices(parent_text);
            
            for (child_rel_offset, child_text) in child_chunks {
                let start_offset = parent_start_offset + child_rel_offset;
                let end_offset = start_offset + child_text.len();

                document_chunks.push(DocumentChunk {
                    id: uuid::Uuid::new_v4().to_string(),
                    document_id: document.id.clone(),
                    tenant_id: document.tenant_id.clone(),
                    content: child_text.to_string(),
                    parent_content: Some(parent_text.to_string()),
                    index: global_index,
                    start_offset,
                    end_offset,
                    metadata: document.metadata.clone(),
                });
                global_index += 1;
            }
        }

        Ok(document_chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::document::Document;
    use common::types::{DocumentId, TenantId};

    #[test]
    fn test_hierarchical_chunker() {
        let chunker = HierarchicalChunker::new(50, 15);
        let content = "This is the first section. And here is more content.\n\nThis is the second section of the document.";
        let doc = Document {
            id: DocumentId("doc-123".to_string()),
            tenant_id: TenantId("tenant-1".to_string()),
            content: content.to_string(),
            title: "Test Doc".to_string(),
            metadata: serde_json::Value::Null,
        };

        let chunks = chunker.chunk(&doc).unwrap();
        assert!(!chunks.is_empty());

        for chunk in &chunks {
            assert_eq!(chunk.document_id.0, "doc-123");
            assert_eq!(chunk.tenant_id.0, "tenant-1");
            
            // Child content should be part of parent content
            assert!(chunk.parent_content.is_some());
            let parent_text = chunk.parent_content.as_ref().unwrap();
            assert!(parent_text.contains(&chunk.content));

            // Offsets should align with the original content
            let sliced_content = &content[chunk.start_offset..chunk.end_offset];
            assert_eq!(sliced_content, chunk.content);
        }
    }
}

