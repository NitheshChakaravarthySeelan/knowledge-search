use anyhow::Result;
use sqlx::postgres::PgPool;

pub struct GraphClient {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct KbNodeRef {
    pub id: uuid::Uuid,
    pub node_type: String,
    pub title: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GraphEdgeRef {
    pub source_id: uuid::Uuid,
    pub target_id: uuid::Uuid,
    pub relation_type: String,
}

#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub document_id: String,
    pub title: String,
    pub relevance: f32,
}

impl GraphClient {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Look up Document nodes whose title contains any of the given entity names.
    pub async fn lookup_nodes_by_entity_name(
        &self,
        entity_names: &[String],
        tenant_id: &str,
    ) -> Result<Vec<KbNodeRef>> {
        if entity_names.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_nodes = Vec::new();
        for name in entity_names {
            let pattern = format!("%{}%", name);
            let rows = sqlx::query_as::<_, (uuid::Uuid, String, Option<String>, Option<String>)>(
                r#"
                SELECT id, node_type, title, file_path
                FROM kb_nodes
                WHERE tenant_id = $1
                  AND node_type = 'Document'
                  AND title ILIKE $2
                LIMIT 5
                "#,
            )
            .bind(tenant_id)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await?;

            for (id, node_type, title, file_path) in rows {
                all_nodes.push(KbNodeRef {
                    id,
                    node_type,
                    title,
                    file_path,
                });
            }
        }

        Ok(all_nodes)
    }

    /// Given a set of node IDs, find all outgoing edges (and their targets).
    /// Optionally also traverse incoming edges.
    pub async fn traverse_edges(
        &self,
        node_ids: &[uuid::Uuid],
        tenant_id: &str,
        max_hops: u32,
    ) -> Result<Vec<GraphEdgeRef>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        let uuids: Vec<uuid::Uuid> = node_ids.to_vec();
        let mut all_edges = Vec::new();

        let rows = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, String)>(
            r#"
            SELECT source_id, target_id, relation_type
            FROM kb_graph_edges
            WHERE tenant_id = $1
              AND (source_id = ANY($2) OR target_id = ANY($2))
            LIMIT 100
            "#,
        )
        .bind(tenant_id)
        .bind(&uuids)
        .fetch_all(&self.pool)
        .await?;

        for (source_id, target_id, relation_type) in rows {
            all_edges.push(GraphEdgeRef {
                source_id,
                target_id,
                relation_type,
            });
        }

        if max_hops > 1 && !all_edges.is_empty() {
            let connected_ids: Vec<uuid::Uuid> = all_edges
                .iter()
                .flat_map(|e| vec![e.source_id, e.target_id])
                .filter(|id| !node_ids.contains(id))
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            if !connected_ids.is_empty() {
                let hop2_rows = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, String)>(
                    r#"
                    SELECT source_id, target_id, relation_type
                    FROM kb_graph_edges
                    WHERE tenant_id = $1
                      AND (source_id = ANY($2) OR target_id = ANY($2))
                    LIMIT 100
                    "#,
                )
                .bind(tenant_id)
                .bind(&connected_ids)
                .fetch_all(&self.pool)
                .await?;

                for (source_id, target_id, relation_type) in hop2_rows {
                    all_edges.push(GraphEdgeRef {
                        source_id,
                        target_id,
                        relation_type,
                    });
                }
            }
        }

        Ok(all_edges)
    }

    /// Collect unique Document node IDs from a list of node IDs by walking
    /// parent_id chains upward until node_type = 'Document'.
    pub async fn resolve_document_ids(
        &self,
        node_ids: &[uuid::Uuid],
        tenant_id: &str,
    ) -> Result<Vec<String>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        let uuids: Vec<uuid::Uuid> = node_ids.to_vec();

        // Get Document nodes directly
        let doc_rows = sqlx::query_as::<_, (uuid::Uuid, String)>(
            r#"
            SELECT id, COALESCE(title, file_path) as doc_title
            FROM kb_nodes
            WHERE tenant_id = $1
              AND id = ANY($2)
              AND node_type = 'Document'
            "#,
        )
        .bind(tenant_id)
        .bind(&uuids)
        .fetch_all(&self.pool)
        .await?;

        // For non-Document nodes, walk parent_id up to find the Document ancestor
        let non_doc_ids: Vec<uuid::Uuid> = node_ids
            .iter()
            .filter(|id| !doc_rows.iter().any(|(did, _)| did == *id))
            .copied()
            .collect();

        let mut document_refs: Vec<String> = doc_rows
            .into_iter()
            .map(|(id, _)| id.to_string())
            .collect();

        if !non_doc_ids.is_empty() {
            let parent_rows = sqlx::query_as::<_, (uuid::Uuid, Option<uuid::Uuid>)>(
                r#"
                SELECT id, parent_id
                FROM kb_nodes
                WHERE tenant_id = $1
                  AND id = ANY($2)
                "#,
            )
            .bind(tenant_id)
            .bind(&non_doc_ids)
            .fetch_all(&self.pool)
            .await?;

            let parent_ids: Vec<uuid::Uuid> = parent_rows
                .into_iter()
                .filter_map(|(_, parent_id)| parent_id)
                .collect();

            if !parent_ids.is_empty() {
                // One more hop up to find Document nodes
                let ancestors = sqlx::query_as::<_, (uuid::Uuid,)>(
                    r#"
                    SELECT id FROM kb_nodes
                    WHERE tenant_id = $1
                      AND id = ANY($2)
                      AND node_type = 'Document'
                    "#,
                )
                .bind(tenant_id)
                .bind(&parent_ids)
                .fetch_all(&self.pool)
                .await?;

                for (id,) in ancestors {
                    document_refs.push(id.to_string());
                }
            }
        }

        document_refs.sort();
        document_refs.dedup();
        Ok(document_refs)
    }

    /// Deletes a Document node and its entire subtree from the knowledge graph.
    ///
    /// This relies on database-level `ON DELETE CASCADE` foreign keys:
    /// - `kb_nodes.parent_id` → `kb_nodes.id` → child AST nodes (Class, Function, etc.)
    ///    are cascade-deleted when their parent Document is removed.
    /// - `kb_graph_edges.source_id` / `target_id` → `kb_nodes.id` → all edges
    ///    referencing the document or any of its children are cascade-deleted.
    ///
    /// Returns `true` if a node was actually deleted, `false` if no matching
    /// Document node was found (already deleted or wrong id).
    pub async fn delete_document_tree(&self, document_id: uuid::Uuid) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM kb_nodes
            WHERE id = $1
              AND node_type = 'Document'
            "#,
        )
        .bind(document_id)
        .execute(&self.pool)
        .await?;

        // rows_affected() tells us whether a Document node was actually found and deleted.
        // If 0, the document either doesn't exist or was already removed.
        let deleted = result.rows_affected() > 0;
        if deleted {
            tracing::info!(
                document_id = document_id.to_string(),
                "Deleted document subtree from knowledge graph (cascade includes child nodes and edges)."
            );
        } else {
            tracing::warn!(
                document_id = document_id.to_string(),
                "No Document node found in knowledge graph for this id (may have been deleted already)."
            );
        }

        Ok(deleted)
    }

    /// Look up a single node by UUID.
    pub async fn get_node(&self, node_id: uuid::Uuid) -> Result<Option<KbNodeRef>> {
        let row = sqlx::query_as::<_, (uuid::Uuid, String, Option<String>, Option<String>)>(
            r#"
            SELECT id, node_type, title, file_path
            FROM kb_nodes
            WHERE id = $1
            "#,
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(id, node_type, title, file_path)| KbNodeRef {
            id,
            node_type,
            title,
            file_path,
        }))
    }
}
