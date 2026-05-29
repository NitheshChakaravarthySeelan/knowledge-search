use reqwest::Client;
use serde::{Deserialize, Serialize};
use common::errors::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotionPage {
    pub id: String,
    pub title: String,
    pub content: String,
    pub last_edited_time: String,
    pub url: String,
}

pub struct NotionClient {
    pub api_token: String,
    client: Client,
}

impl NotionClient {
    pub fn new(api_token: String) -> Self {
        Self {
            api_token,
            client: Client::new(),
        }
    }

    /// Fetches all pages available in the Notion workspace.
    pub async fn fetch_pages(&self) -> Result<Vec<NotionPage>> {
        if self.api_token.is_empty() || self.api_token == "mock" {
            // High fidelity sandbox developer mock data
            return Ok(vec![
                NotionPage {
                    id: "notion_page_101".to_string(),
                    title: "Engineering Onboarding Roadmap".to_string(),
                    content: "# Engineering Onboarding\n\nWelcome to Knowledge-OS! This document outlines our high-performance Rust monorepo context. We use Cargo Workspace and Bun.".to_string(),
                    last_edited_time: "2026-05-29T11:45:00Z".to_string(),
                    url: "https://notion.so/knowledge-os/Engineering-Onboarding-Roadmap-notion_page_101".to_string(),
                },
                NotionPage {
                    id: "notion_page_102".to_string(),
                    title: "Database Strategy Draft".to_string(),
                    content: "# Database Strategy\n\nOur system operates on Postgres for traditional metadata tracking and transactional entities, and Qdrant for storing and querying text vector embeddings.".to_string(),
                    last_edited_time: "2026-05-29T11:47:00Z".to_string(),
                    url: "https://notion.so/knowledge-os/Database-Strategy-Draft-notion_page_102".to_string(),
                }
            ]);
        }

        // Notion API search request
        let response = self.client
            .post("https://api.notion.com/v1/search")
            .bearer_auth(&self.api_token)
            .header("Notion-Version", "2022-06-28")
            .json(&serde_json::json!({
                "filter": {
                    "value": "page",
                    "property": "object"
                }
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "Notion".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalService {
                service: "Notion".to_string(),
                message: format!("Status: {}, Body: {}", status, body),
            });
        }

        let json_body: serde_json::Value = response.json().await.map_err(|e| AppError::ExternalService {
            service: "Notion".to_string(),
            message: format!("Failed to parse response body as JSON: {}", e),
        })?;
        let results = json_body["results"]
            .as_array()
            .ok_or_else(|| AppError::ExternalService {
                service: "Notion".to_string(),
                message: "No results array in Notion API response".to_string(),
            })?;

        let mut pages = Vec::new();
        for item in results {
            let id = item["id"].as_str().unwrap_or_default().to_string();
            let last_edited = item["last_edited_time"].as_str().unwrap_or_default().to_string();
            let url = item["url"].as_str().unwrap_or_default().to_string();

            // Safely retrieve title from Notion structure
            let title = item["properties"]["title"]["title"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|first| first["plain_text"].as_str())
                .unwrap_or("Untitled Page")
                .to_string();

            pages.push(NotionPage {
                id,
                title,
                content: format!("Notion Page Title: {}\n(Content synchronizes via background Sync Worker).", url),
                last_edited_time: last_edited,
                url,
            });
        }

        Ok(pages)
    }
}
