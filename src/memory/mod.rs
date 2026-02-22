pub mod embedding;
pub mod embedders;
pub mod error;
pub mod manager;
pub mod scoring;
pub mod store;
pub mod stores;
pub mod types;

pub use embedding::EmbeddingProvider;
pub use embedders::ollama::OllamaEmbedder;
pub use error::MemoryError;
pub use manager::{EpisodicMemory, GcResult};
pub use scoring::DecayConfig;
pub use store::{EvictionCandidate, MemoryStore};
#[cfg(feature = "qdrant")]
pub use stores::qdrant::QdrantStore;
pub use types::{
    Confidence, ContentType, IngestRequest, MemoryEntry, MemoryHit, RequiredMetadata,
};
