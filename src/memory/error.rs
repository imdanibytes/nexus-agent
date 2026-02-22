#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("store error: {0}")]
    Store(String),
    #[error("embedding error: {0}")]
    Embedding(String),
    #[error("entry not found: {0}")]
    NotFound(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}
