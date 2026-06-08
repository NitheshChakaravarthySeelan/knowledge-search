use common::config::AppConfig;
use embeddings::providers::NvidiaProvider;
use embeddings::traits::EmbeddingProvider;
use embeddings::models::EmbeddingInput;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load config from environment
    // dotenvy is already used by AppConfig::load_from_env, 
    // so we don't need to call dotenvy::dotenv() here.
    let config = AppConfig::load_from_env()?;

    let api_key = config.nvidia_api_key.ok_or_else(|| {
        anyhow::anyhow!("NVIDIA_API_KEY not found in environment")
    })?;

    println!("Testing NVIDIA Embedding Provider...");
    let provider = NvidiaProvider::new(api_key);
    
    let input = EmbeddingInput {
        text: "This is a test to verify NVIDIA embedding API connectivity.".to_string(),
        user_id: None,
    };

    let result = provider.embed(&input).await;

    match result {
        Ok(embedding) => {
            println!("Successfully generated embedding!");
            println!("Vector dimensions: {}", embedding.dimensions);
            println!("First 5 values: {:?}", &embedding.vector[..5]);
        }
        Err(e) => {
            eprintln!("Failed to generate embedding: {:?}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
