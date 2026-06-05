pub mod models;
pub mod providers;
pub mod traits;
pub mod sparse;

pub use models::{Embedding, EmbeddingInput};
pub use traits::EmbeddingProvider;
pub use providers::{OpenAiProvider, GeminiProvider, NvidiaProvider};
pub use sparse::{SparseVector, SparseEmbeddingProvider, LocalHashingSparseEncoder};
