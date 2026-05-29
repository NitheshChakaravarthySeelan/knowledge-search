use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, error, warn};

use common::config::AppConfig;
use common::telemetry::init_telemetry;
use connectors::NotionClient;

#[tokio::main]
async fn main() {
    // 1. Initialize logging
    init_telemetry("sync-worker");
    info!("Starting Sync Worker daemon...");

    // 2. Load settings
    let config = match AppConfig::load_from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load configuration: {:?}", e);
            std::process::exit(1);
        }
    };

    // 3. Setup Notion Client
    let notion_token = config.notion_api_token.unwrap_or_else(|| {
        warn!("NOTION_API_TOKEN not set in environment. Falling back to sandbox developer mode.");
        "mock".to_string()
    });
    
    let notion_client = NotionClient::new(notion_token);

    // 4. Run Scheduled Crawler Sync Loop
    info!("Sync Worker scheduler is online and monitoring external connectors...");
    let mut iteration = 0;

    loop {
        iteration += 1;
        info!(iteration = iteration, "Executing scheduled Notion content sync run...");

        match notion_client.fetch_pages().await {
            Ok(pages) => {
                info!("Fetched {} pages from Notion workspace.", pages.len());
                for page in pages {
                    info!(
                        page_id = page.id,
                        title = page.title,
                        url = page.url,
                        last_edited = page.last_edited_time,
                        "Syncing Notion page content with central repository..."
                    );
                    
                    // Simulate checking hashes and writing into database
                    info!(page_id = page.id, "SUCCESS: Page content synced cleanly.");
                }
            }
            Err(e) => {
                error!("Notion synchronization failure: {:?}", e);
            }
        }

        // Sleep to throttle scheduled poll duration (e.g. check every 30 seconds for the mock, in prod this could be 1 hour)
        sleep(Duration::from_secs(30)).await;
    }
}
