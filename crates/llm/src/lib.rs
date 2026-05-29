pub mod providers;

use async_trait::async_trait;
use common::errors::Result;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generates text completion using the large language model.
    async fn generate(&self, prompt: &str, system_instruction: Option<&str>) -> Result<String>;
}

pub use providers::{OpenAiLlm, GeminiLlm};
