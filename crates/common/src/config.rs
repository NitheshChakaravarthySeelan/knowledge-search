use std::env;
use serde::{Deserialize, Serialize};
use crate::errors::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub environment: String,
    pub database_url: String,
    pub qdrant_url: String,
    pub notion_api_token: Option<String>,
    pub openai_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub nvidia_api_key: Option<String>,
}

impl AppConfig {
    /// Loads configuration parameters from the environment.
    pub fn load_from_env() -> Result<Self> {
        // Load .env file if it exists
        dotenvy::dotenv().ok();

        let environment = env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());
        
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/knowledge_os".to_string());
        
        let qdrant_url = env::var("QDRANT_URL")
            .unwrap_or_else(|_| "http://localhost:6334".to_string());

        let notion_api_token = env::var("NOTION_API_TOKEN").ok();
        let openai_api_key = env::var("OPENAI_API_KEY").ok();
        let gemini_api_key = env::var("GEMINI_API_KEY").ok();
        let nvidia_api_key = env::var("NVIDIA_API_KEY").ok();

        Ok(Self {
            environment,
            database_url,
            qdrant_url,
            notion_api_token,
            openai_api_key,
            gemini_api_key,
            nvidia_api_key,
        })
    }
}
