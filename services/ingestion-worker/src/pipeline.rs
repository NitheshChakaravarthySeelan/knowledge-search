use std::collections::HashMap;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use sha2::{Sha256, Digest};
use uuid::Uuid;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
};
use tracing::{info, debug, warn};

use common::types::TenantId;
use documents::chunkers::{Chunker, HierarchicalChunker};
use documents::{GraphExtractor, ExtractedNode};
use embeddings::{EmbeddingProvider, SparseEmbeddingProvider, EmbeddingInput};
use connectors::QdrantClient;
use entities::{kb_node, kb_graph_edge, document_job};

pub struct IngestionPipeline {
    db: DatabaseConnection,
    qdrant: Arc<QdrantClient>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    sparse_provider: Arc<dyn SparseEmbeddingProvider>,
    chunker: Arc<dyn Chunker>,
}

impl IngestionPipeline {
    pub fn new(
        db: DatabaseConnection,
        qdrant: Arc<QdrantClient>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        sparse_provider: Arc<dyn SparseEmbeddingProvider>,
    ) -> Self {
        Self {
            db,
            qdrant,
            embedding_provider,
            sparse_provider,
            chunker: Arc::new(HierarchicalChunker::default()),
        }
    }

    /// Orchestrates the full ingestion flow:
    ///   1. SHA-256 change detection (skip if unchanged)
    ///   2. Postgres graph: Document node + AST child nodes + graph edges (with Placeholder self-healing)
    ///   3. Qdrant: Hierarchical text chunks (dense + sparse)
    ///   4. Qdrant: AST child node embeddings (precise function/class-level search)
    pub async fn process_job(
        &self,
        job: &document_job::Model,
        tenant_id: &TenantId,
        source_type: &str,
        file_path: &str,   // Stable identifier for deduplication (e.g. "src/lib.rs" or upload UUID)
        title: &str,       // Human-readable document title
        content: &str,
        file_ext: Option<&str>,
        collection_name: &str,
    ) -> Result<()> {
        self.update_job_status(job, 1, 10, "Extracting and Parsing...").await?;
        
        // ── Step 1: SHA256 Content Hash ────────────────────────────────────────────
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        // ── Step 2: Postgres Graph — atomic transaction ────────────────────────────
        self.update_job_status(job, 2, 25, "Upserting Graph...").await?;
        let doc_uuid = self.upsert_graph(
            tenant_id,
            source_type,
            file_path,
            title,
            content,
            file_ext,
            &content_hash,
        ).await?;

        // doc_uuid == None means content is unchanged; skip all downstream work.
        let doc_uuid = match doc_uuid {
            Some(id) => id,
            None => {
                self.update_job_status(job, 4, 100, "Content unchanged (skipped).").await?;
                return Ok(());
            }
        };

        // ── Step 3: Hierarchical Text Chunks → Qdrant ──────────────────────────────
        self.update_job_status(job, 3, 50, "Embedding and Indexing Chunks...").await?;
        self.embed_and_upsert_chunks(
            tenant_id,
            doc_uuid,
            title,
            content,
            collection_name,
        ).await?;

        // ── Step 4: AST Child Nodes → Qdrant ──────────────────────────────────────
        self.update_job_status(job, 4, 90, "Embedding AST Nodes...").await?;
        let graph_data = GraphExtractor::extract(file_path, content, file_ext);
        if !graph_data.children.is_empty() {
            self.embed_and_upsert_ast_nodes(
                tenant_id,
                doc_uuid,
                &graph_data.children,
                collection_name,
            ).await?;
        }
        self.update_job_status(job, 4, 100, "Ingestion complete.").await?;

        info!(
            tenant = tenant_id.0,
            file_path = file_path,
            "Ingestion complete: graph + chunks + AST nodes all indexed."
        );

        Ok(())
    }

    async fn update_job_status(
        &self,
        job: &document_job::Model,
        stage: i32,
        percent: i32,
        message: &str,
    ) -> Result<()> {
        let mut active_job: document_job::ActiveModel = job.clone().into();
        active_job.progress_stage = ActiveValue::Set(Some(stage));
        active_job.progress_percent = ActiveValue::Set(Some(percent));
        active_job.progress_message = ActiveValue::Set(Some(message.to_string()));
        if stage == 1 {
            active_job.started_at = ActiveValue::Set(Some(chrono::Utc::now().naive_utc()));
        }
        if stage == 4 && percent == 100 {
             active_job.completed_at = ActiveValue::Set(Some(chrono::Utc::now().naive_utc()));
        }
        active_job.update(&self.db).await?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────────
    //  PRIVATE: Graph upsert (Postgres transaction)
    // ─────────────────────────────────────────────────────────────────────────────

    /// Returns `Some(doc_uuid)` if the document was inserted/updated, `None` if unchanged.
    async fn upsert_graph(
        &self,
        tenant_id: &TenantId,
        source_type: &str,
        file_path: &str,
        title: &str,
        content: &str,
        file_ext: Option<&str>,
        content_hash: &str,
    ) -> Result<Option<Uuid>> {
        let txn = self.db.begin().await?;

        // ── 2a. Change Detection ──────────────────────────────────────────────────
        // Look up any existing Document node for this file_path (stable dedup key).
        let existing_doc = kb_node::Entity::find()
            .filter(kb_node::Column::TenantId.eq(&tenant_id.0))
            .filter(kb_node::Column::FilePath.eq(file_path))
            .filter(kb_node::Column::NodeType.eq("Document"))
            .one(&txn)
            .await?;

        if let Some(ref doc) = existing_doc {
            if doc.content_hash == content_hash {
                info!(
                    tenant = tenant_id.0,
                    file_path = file_path,
                    "Content unchanged (hash match). Skipping ingestion."
                );
                txn.commit().await?;
                return Ok(None);
            }

            // Content changed → delete old node; FK CASCADE removes all child nodes
            // and both inbound/outbound edges automatically.
            info!(
                tenant = tenant_id.0,
                file_path = file_path,
                "Content changed. Deleting stale node tree (cascade)."
            );
            kb_node::Entity::delete_by_id(doc.id).exec(&txn).await?;
        }

        // ── 2b. Placeholder Promotion ─────────────────────────────────────────────
        // If another document linked to this title before it was ingested, a
        // Placeholder node already exists. We reuse its UUID so all back-links survive.
        let maybe_placeholder = kb_node::Entity::find()
            .filter(kb_node::Column::TenantId.eq(&tenant_id.0))
            .filter(kb_node::Column::Title.eq(title))
            .filter(kb_node::Column::NodeType.eq("Placeholder"))
            .one(&txn)
            .await?;

        let doc_uuid = match &maybe_placeholder {
            Some(ph) => {
                info!(
                    tenant = tenant_id.0,
                    title = title,
                    uuid = ph.id.to_string(),
                    "Promoting Placeholder → Document (preserving all back-links)."
                );
                ph.id
            }
            None => Uuid::new_v4(),
        };

        // ── 2c. Upsert Primary Document Node ──────────────────────────────────────
        let doc_model = kb_node::ActiveModel {
            id:             ActiveValue::Set(doc_uuid),
            parent_id:      ActiveValue::Set(None),
            tenant_id:      ActiveValue::Set(tenant_id.0.clone()),
            source_type:    ActiveValue::Set(source_type.to_string()),
            file_path:      ActiveValue::Set(file_path.to_string()),
            node_type:      ActiveValue::Set("Document".to_string()),
            title:          ActiveValue::Set(Some(title.to_string())),
            content:        ActiveValue::Set(content.to_string()),
            parent_content: ActiveValue::Set(None),
            content_hash:   ActiveValue::Set(content_hash.to_string()),
            metadata:       ActiveValue::Set(None),
            created_at:     ActiveValue::Set(chrono::Utc::now().into()),
        };

        if maybe_placeholder.is_some() {
            // Promote existing placeholder row in-place
            doc_model.update(&txn).await?;
        } else {
            doc_model.insert(&txn).await?;
        }

        // ── 2d. AST / Graph Extraction ────────────────────────────────────────────
        let graph_data = GraphExtractor::extract(file_path, content, file_ext);

        // Build a name→UUID map for resolving local references inside this document.
        let mut name_to_uuid: HashMap<String, Uuid> = HashMap::new();
        name_to_uuid.insert(title.to_string(), doc_uuid);
        name_to_uuid.insert(file_path.to_string(), doc_uuid);

        // ── 2e. Insert Structural Child Nodes (Classes, Functions, Sections) ───────
        for child in &graph_data.children {
            let child_uuid = Uuid::new_v4();
            name_to_uuid.insert(child.name.clone(), child_uuid);

            let child_model = kb_node::ActiveModel {
                id:             ActiveValue::Set(child_uuid),
                parent_id:      ActiveValue::Set(Some(doc_uuid)),
                tenant_id:      ActiveValue::Set(tenant_id.0.clone()),
                source_type:    ActiveValue::Set(source_type.to_string()),
                file_path:      ActiveValue::Set(file_path.to_string()),
                node_type:      ActiveValue::Set(child.node_type.clone()),
                title:          ActiveValue::Set(Some(child.name.clone())),
                content:        ActiveValue::Set(child.content.clone()),
                parent_content: ActiveValue::Set(Some(content.to_string())),
                content_hash:   ActiveValue::Set(content_hash.to_string()),
                metadata:       ActiveValue::Set(None),
                created_at:     ActiveValue::Set(chrono::Utc::now().into()),
            };

            child_model.insert(&txn).await?;
        }

        // ── 2f. Insert Graph Edges (with Placeholder self-healing) ─────────────────
        for edge in &graph_data.edges {
            let source_uuid = match name_to_uuid.get(&edge.source_node_name) {
                Some(id) => *id,
                None => {
                    debug!(
                        source = edge.source_node_name,
                        "Source node not in local map, skipping edge."
                    );
                    continue;
                }
            };

            let target_uuid = if let Some(uuid) = name_to_uuid.get(&edge.target_node_name) {
                *uuid
            } else {
                // Check if the target already exists anywhere in the KB.
                let existing_target = kb_node::Entity::find()
                    .filter(kb_node::Column::TenantId.eq(&tenant_id.0))
                    .filter(
                        kb_node::Column::Title.eq(&edge.target_node_name)
                            .or(kb_node::Column::FilePath.eq(&edge.target_node_name)),
                    )
                    .one(&txn)
                    .await?;

                if let Some(t) = existing_target {
                    t.id
                } else {
                    // Self-healing: create a Placeholder so the edge can be stored now.
                    // When the target document is eventually ingested it will promote this
                    // Placeholder to a real Document node, preserving all edges.
                    let ph_uuid = Uuid::new_v4();
                    info!(
                        tenant = tenant_id.0,
                        target = edge.target_node_name,
                        "Target not found — creating Placeholder node."
                    );

                    let ph_model = kb_node::ActiveModel {
                        id:             ActiveValue::Set(ph_uuid),
                        parent_id:      ActiveValue::Set(None),
                        tenant_id:      ActiveValue::Set(tenant_id.0.clone()),
                        source_type:    ActiveValue::Set(source_type.to_string()),
                        file_path:      ActiveValue::Set(edge.target_node_name.clone()),
                        node_type:      ActiveValue::Set("Placeholder".to_string()),
                        title:          ActiveValue::Set(Some(edge.target_node_name.clone())),
                        content:        ActiveValue::Set(String::new()),
                        parent_content: ActiveValue::Set(None),
                        content_hash:   ActiveValue::Set(String::new()),
                        metadata:       ActiveValue::Set(None),
                        created_at:     ActiveValue::Set(chrono::Utc::now().into()),
                    };

                    ph_model.insert(&txn).await?;
                    name_to_uuid.insert(edge.target_node_name.clone(), ph_uuid);
                    ph_uuid
                }
            };

            let edge_model = kb_graph_edge::ActiveModel {
                source_id:     ActiveValue::Set(source_uuid),
                target_id:     ActiveValue::Set(target_uuid),
                relation_type: ActiveValue::Set(edge.relation_type.clone()),
                tenant_id:     ActiveValue::Set(tenant_id.0.clone()),
                metadata:      ActiveValue::Set(None),
            };

            // Silently ignore duplicate edges (composite PK on source+target+relation).
            if let Err(e) = edge_model.insert(&txn).await {
                debug!(error = e.to_string(), "Edge already exists or constraint violation, skipping.");
            }
        }

        txn.commit().await?;

        info!(
            tenant = tenant_id.0,
            file_path = file_path,
            doc_uuid = doc_uuid.to_string(),
            "Graph upsert committed to Postgres."
        );

        Ok(Some(doc_uuid))
    }

    // ─────────────────────────────────────────────────────────────────────────────
    //  PRIVATE: Hierarchical text chunking → Qdrant
    // ─────────────────────────────────────────────────────────────────────────────

    async fn embed_and_upsert_chunks(
        &self,
        tenant_id: &TenantId,
        doc_uuid: Uuid,
        title: &str,
        content: &str,
        collection_name: &str,
    ) -> Result<()> {
        let doc_to_chunk = documents::models::document::Document {
            id: common::types::DocumentId(doc_uuid.to_string()),
            tenant_id: tenant_id.clone(),
            source_type: common::types::SourceType::FileUpload,
            title: title.to_string(),
            content: content.to_string(),
            metadata: serde_json::Value::Null,
            version: 1,
        };

        let chunks = self.chunker.chunk(&doc_to_chunk)
            .map_err(|e| anyhow!("Chunking failed: {:?}", e))?;

        info!(
            tenant = tenant_id.0,
            title = title,
            count = chunks.len(),
            "Hierarchical chunks generated."
        );

        // Process chunks in batches of 50
        let provider = self.embedding_provider.clone();
        let sparse = self.sparse_provider.clone();
        
        let processed_data = common::batch_utils::process_in_batches(&chunks, 50, |batch| {
            let provider = provider.clone();
            let sparse = sparse.clone();
            let batch = batch.to_vec();
            
            async move {
                let mut results = Vec::with_capacity(batch.len());
                let inputs: Vec<EmbeddingInput> = batch.iter()
                    .map(|c| EmbeddingInput { text: c.content.clone(), user_id: None })
                    .collect();

                let dense = provider.embed_batch(&inputs).await?;
                
                for (i, chunk) in batch.into_iter().enumerate() {
                    let s = sparse.embed_sparse(&chunk.content).await?;
                    results.push((chunk, dense[i].vector.clone(), (s.indices, s.values)));
                }

                Ok(results)
            }
        }).await?;

        // Merge results
        let mut doc_chunks = Vec::with_capacity(processed_data.len());
        let mut dense_vecs = Vec::with_capacity(processed_data.len());
        let mut sparse_vecs = Vec::with_capacity(processed_data.len());

        for (chunk, dense, sparse) in processed_data {
            doc_chunks.push(chunk);
            dense_vecs.push(dense);
            sparse_vecs.push(sparse);
        }

        if !doc_chunks.is_empty() {
            self.qdrant.upsert_chunks_hybrid(
                collection_name,
                &doc_chunks,
                &dense_vecs,
                &sparse_vecs,
            ).await?;

            info!(
                tenant = tenant_id.0,
                count = doc_chunks.len(),
                "Text chunks upserted to Qdrant."
            );
        }

        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────────
    //  PRIVATE: AST child node embeddings → Qdrant
    // ─────────────────────────────────────────────────────────────────────────────

    async fn embed_and_upsert_ast_nodes(
        &self,
        tenant_id: &TenantId,
        doc_uuid: Uuid,
        children: &[ExtractedNode],
        collection_name: &str,
    ) -> Result<()> {
        let mut ast_chunks  = Vec::with_capacity(children.len());
        let mut dense_vecs  = Vec::with_capacity(children.len());
        let mut sparse_vecs = Vec::with_capacity(children.len());

        for child in children {
            let input = EmbeddingInput {
                text: child.content.clone(),
                user_id: None,
            };

            let dense  = self.embedding_provider.embed(&input).await
                .map_err(|e| {
                    warn!(node = child.name, "Dense embed failed for AST node: {:?}", e);
                    e
                })?;
            let sparse = self.sparse_provider.embed_sparse(&child.content).await?;

            let chunk = documents::models::document_chunk::DocumentChunk {
                id: Uuid::new_v4().to_string(),
                document_id: common::types::DocumentId(doc_uuid.to_string()),
                tenant_id: tenant_id.clone(),
                content: child.content.clone(),
                parent_content: Some(format!("[{}] {}", child.node_type, child.name)),
                index: child.start_offset,
                start_offset: child.start_offset,
                end_offset: child.end_offset,
                metadata: serde_json::json!({
                    "node_type": child.node_type,
                    "node_name": child.name,
                    "is_ast_node": true,
                }),
            };

            ast_chunks.push(chunk);
            dense_vecs.push(dense.vector);
            sparse_vecs.push((sparse.indices, sparse.values));
        }

        if !ast_chunks.is_empty() {
            self.qdrant.upsert_chunks_hybrid(
                collection_name,
                &ast_chunks,
                &dense_vecs,
                &sparse_vecs,
            ).await?;

            info!(
                tenant = tenant_id.0,
                count = ast_chunks.len(),
                "AST child nodes upserted to Qdrant."
            );
        }

        Ok(())
    }
}
