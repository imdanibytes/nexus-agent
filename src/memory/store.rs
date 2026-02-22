use async_trait::async_trait;

use super::error::MemoryError;
use super::types::{MemoryEntry, MemoryHit};

/// Backend storage trait. Qdrant, ChromaDB, SQLite+vectors, whatever —
/// implement this and plug it in. The memory manager handles lifecycle;
/// the store just stores and retrieves.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Store an entry. The entry already has its embedding computed.
    /// Returns the assigned ID (may differ from entry.id if the store generates its own).
    async fn upsert(&self, entry: &MemoryEntry) -> Result<String, MemoryError>;

    /// Semantic search by embedding vector. Returns top `limit` hits
    /// ordered by descending relevance. Only returns summary-level data.
    async fn search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<MemoryHit>, MemoryError>;

    /// Fetch the full entry by ID — content, embedding, everything.
    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError>;

    /// Delete an entry permanently.
    async fn delete(&self, id: &str) -> Result<(), MemoryError>;

    /// Bump access stats: increment access_count, update last_accessed_at.
    async fn touch(&self, id: &str) -> Result<(), MemoryError>;

    /// Return IDs of all unpinned entries with a decay score below `threshold`.
    /// The store doesn't compute the score — caller provides the threshold
    /// and the store filters on `last_accessed_at`, `access_count`, and `pinned`.
    /// Returns (id, last_accessed_at, access_count) tuples for the manager to score.
    async fn list_candidates(&self) -> Result<Vec<EvictionCandidate>, MemoryError>;

    /// Total number of entries in the store.
    async fn count(&self) -> Result<usize, MemoryError>;

    /// Pin or unpin an entry. Pinned entries are exempt from eviction.
    async fn set_pinned(&self, id: &str, pinned: bool) -> Result<(), MemoryError>;
}

/// Lightweight struct returned by `list_candidates` for eviction scoring.
#[derive(Debug, Clone)]
pub struct EvictionCandidate {
    pub id: String,
    pub last_accessed_at: chrono::DateTime<chrono::Utc>,
    pub access_count: u32,
    pub confidence: super::types::Confidence,
    pub pinned: bool,
}
