use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::conversation::types::Conversation;

/// In-memory conversation cache. Unbounded HashMap behind RwLock.
pub struct ThreadCache {
    inner: RwLock<HashMap<String, Conversation>>,
}

impl ThreadCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Get a clone of a cached conversation.
    pub async fn get(&self, id: &str) -> Option<Conversation> {
        self.inner.read().await.get(id).cloned()
    }

    /// Insert or replace a conversation in the cache.
    pub async fn insert(&self, id: String, conv: Conversation) {
        self.inner.write().await.insert(id, conv);
    }

    /// Remove a conversation from the cache.
    pub async fn invalidate(&self, id: &str) {
        self.inner.write().await.remove(id);
    }
}
