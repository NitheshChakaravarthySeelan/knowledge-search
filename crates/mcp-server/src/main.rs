use anyhow::Result;
use connectors::QdrantClient;
use embeddings::providers::NvidiaProvider;
use embeddings::sparse::LocalHashingSparseEncoder;
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router, ServiceExt, transport::stdio};
use schemars::JsonSchema;
use search::{HybridRetriever, LocalReranker, SearchService};
use serde::Deserialize;
use std::sync::Arc;
use std::process::Command;

#[derive(Clone)]
struct MyServer {
    search_service: Arc<SearchService>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchParams {
    /// The search query.
    query: String,
    /// The tenant ID (defaults to "default").
    tenant_id: Option<String>,
    /// Number of results (default 5).
    limit: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct IngestParams {
    path: String,
}

#[tool_router(server_handler)]
impl MyServer {
    #[tool(description = "Performs a hybrid search over the knowledge base.")]
    async fn search_knowledge_base(&self, Parameters(params): Parameters<SearchParams>) -> String {
        eprintln!("DEBUG: Rust MCP server received search request for query: {}", params.query);
        
        let tenant_id_str = params.tenant_id.unwrap_or_else(|| "default".to_string());
        let tenant_id = common::types::TenantId(tenant_id_str);
        let limit = params.limit.unwrap_or(5) as usize;

        match self
            .search_service
            .search(&tenant_id, &params.query, limit)
            .await
        {
            Ok(results) => {
                eprintln!("DEBUG: Rust MCP search successful.");
                serde_json::to_string(&results).unwrap_or_else(|e| e.to_string())
            },
            Err(e) => {
                eprintln!("DEBUG: Rust MCP search error: {}", e);
                format!("Error: {}", e)
            },
        }
    }

    #[tool(description = "Ingests a PDF file and returns its markdown representation.")]
    async fn ingest_pdf(&self, Parameters(params): Parameters<IngestParams>) -> String {
        let output = Command::new("python3")
            .arg("scripts/ingest.py")
            .arg(&params.path)
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    String::from_utf8_lossy(&out.stdout).to_string()
                } else {
                    format!("Error: {}", String::from_utf8_lossy(&out.stderr))
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = common::config::AppConfig::load_from_env()?;

    let embedding_provider = Arc::new(NvidiaProvider::new(
        config.nvidia_api_key.unwrap_or_default(),
    ));
    let sparse_provider = Arc::new(LocalHashingSparseEncoder::default());
    let qdrant_client = Arc::new(QdrantClient::new(&config.qdrant_url)?);

    let retriever = Arc::new(HybridRetriever::new(
        embedding_provider,
        sparse_provider,
        qdrant_client,
        "knowledge_base".to_string(),
    ));

    let reranker = Arc::new(LocalReranker);
    let search_service = Arc::new(SearchService::new(retriever, reranker));

    let service = MyServer { search_service };
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    
    Ok(())
}
