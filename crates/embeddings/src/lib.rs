pub mod models;
pub mod providers;
pub mod traits;

pub use models::{Embedding, EmbeddingInput};
pub use traits::EmbeddingProvider;
pub use providers::{OpenAiProvider, GeminiProvider};
