use chrono::Utc;
use tracing::{info, warn};

use super::embedding::EmbeddingProvider;
use super::error::MemoryError;
use super::scoring::{select_evictions, DecayConfig};
use super::store::MemoryStore;
use super::types::{IngestRequest, MemoryEntry, MemoryHit};

/// Episodic memory manager. Wires together a store and an embedding provider,
/// handles the full lifecycle: ingest, retrieve, touch, evict.
pub struct EpisodicMemory {
    store: Box<dyn MemoryStore>,
    embedder: Box<dyn EmbeddingProvider>,
    decay_config: DecayConfig,
}

impl EpisodicMemory {
    pub fn new(
        store: impl MemoryStore + 'static,
        embedder: impl EmbeddingProvider + 'static,
    ) -> Self {
        Self {
            store: Box::new(store),
            embedder: Box::new(embedder),
            decay_config: DecayConfig::default(),
        }
    }

    pub fn with_decay_config(mut self, config: DecayConfig) -> Self {
        self.decay_config = config;
        self
    }

    /// Ingest new content into memory. Computes embedding, generates ID,
    /// stores the full entry. Returns the entry ID.
    pub async fn ingest(&self, request: IngestRequest) -> Result<String, MemoryError> {
        let embedding = self.embedder.embed(&request.content).await?;
        let id = generate_id();
        let now = Utc::now();

        let entry = MemoryEntry {
            id: id.clone(),
            summary: request.metadata.title.clone(),
            content: request.content,
            embedding,
            metadata: request.metadata,
            confidence: request.confidence,
            pinned: false,
            created_at: now,
            last_accessed_at: now,
            access_count: 0,
        };

        let stored_id = self.store.upsert(&entry).await?;
        info!(id = %stored_id, source = %entry.metadata.source, "ingested memory entry");
        Ok(stored_id)
    }

    /// Semantic search. Returns top-k lightweight hits (summaries + IDs).
    /// This is what gets injected into the agent's context window.
    pub async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryHit>, MemoryError> {
        let query_embedding = self.embedder.embed(query).await?;
        let hits = self.store.search(&query_embedding, limit).await?;

        // Touch everything that was returned — it's being "accessed"
        for hit in &hits {
            if let Err(e) = self.store.touch(&hit.id).await {
                warn!(id = %hit.id, error = %e, "failed to touch memory entry");
            }
        }

        Ok(hits)
    }

    /// Fetch full content for a specific entry. Used when the model decides
    /// a summary isn't enough and calls fetch_full(id).
    pub async fn fetch_full(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        if let Some(entry) = self.store.get(id).await? {
            self.store.touch(id).await?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    /// Pin an entry so it's never evicted.
    pub async fn pin(&self, id: &str) -> Result<(), MemoryError> {
        self.store.set_pinned(id, true).await
    }

    /// Unpin an entry, making it subject to normal eviction rules.
    pub async fn unpin(&self, id: &str) -> Result<(), MemoryError> {
        self.store.set_pinned(id, false).await
    }

    /// Run garbage collection. Scores all entries, evicts those below threshold
    /// or over the max entry budget. Call this on a schedule (daily is fine).
    pub async fn gc(&self) -> Result<GcResult, MemoryError> {
        let candidates = self.store.list_candidates().await?;
        let total = self.store.count().await?;
        let now = Utc::now();

        let to_evict = select_evictions(&candidates, total, now, &self.decay_config);
        let evicted_count = to_evict.len();

        for id in &to_evict {
            if let Err(e) = self.store.delete(id).await {
                warn!(id = %id, error = %e, "failed to evict memory entry");
            }
        }

        info!(
            total_before = total,
            evicted = evicted_count,
            remaining = total - evicted_count,
            "memory gc complete"
        );

        Ok(GcResult {
            total_before: total,
            evicted: evicted_count,
            remaining: total - evicted_count,
        })
    }

    /// How many entries are currently stored.
    pub async fn count(&self) -> Result<usize, MemoryError> {
        self.store.count().await
    }
}

/// Result of a garbage collection run.
#[derive(Debug, Clone)]
pub struct GcResult {
    pub total_before: usize,
    pub evicted: usize,
    pub remaining: usize,
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("mem_{ts:x}")
}
