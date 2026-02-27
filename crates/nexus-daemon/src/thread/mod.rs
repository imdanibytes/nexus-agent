mod cache;
pub mod store;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::RwLock;

use crate::conversation::types::{ChatMessage, Conversation, ConversationMeta, ConversationUsage};
use crate::conversation::ConversationStore;
use crate::event_bus::EventBus;

use cache::ThreadCache;

/// Encapsulated service for conversation (thread) CRUD.
///
/// - Manages its own locking — callers never see RwLock.
/// - Reads hit cache first, fall through to the store.
/// - Mutations update cache, emit events, and persist.
pub struct ThreadService {
    store: RwLock<ConversationStore>,
    cache: ThreadCache,
    event_bus: EventBus,
}

impl ThreadService {
    pub fn new(store: ConversationStore, event_bus: EventBus) -> Self {
        Self {
            store: RwLock::new(store),
            cache: ThreadCache::new(),
            event_bus,
        }
    }

    /// Access to the EventBus (for callers that need to emit non-data events).
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    // ── Reads ──

    pub async fn list(&self) -> Vec<ConversationMeta> {
        let store = self.store.read().await;
        store.list().to_vec()
    }

    /// Get a conversation by ID. Cache-first, falls through to disk.
    pub async fn get(&self, id: &str) -> Result<Option<Conversation>> {
        // Check cache first
        if let Some(conv) = self.cache.get(id).await {
            return Ok(Some(conv));
        }

        // Fall through to store
        let store = self.store.read().await;
        let conv = store.get(id).context("failed to load conversation")?;

        // Populate cache on hit
        if let Some(ref c) = conv {
            self.cache.insert(c.id.clone(), c.clone()).await;
        }

        Ok(conv)
    }

    /// Build Anthropic API messages for a conversation.
    pub async fn build_api_messages(
        &self,
        id: &str,
    ) -> Result<Vec<crate::anthropic::types::Message>> {
        let conv = self
            .get(id)
            .await?
            .context("conversation not found")?;
        Ok(conv.build_api_messages())
    }

    // ── Writes ──

    pub async fn create(&self, client_id: Option<String>) -> Result<ConversationMeta> {
        let mut store = self.store.write().await;
        let meta = store.create(client_id)?;

        self.event_bus.emit_data(
            &meta.id,
            "thread_created",
            serde_json::json!({ "id": &meta.id, "title": &meta.title }),
        );

        Ok(meta)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut store = self.store.write().await;
        store.delete(id)?;
        drop(store);

        self.cache.invalidate(id).await;

        self.event_bus.emit_data(
            id,
            "thread_deleted",
            serde_json::json!({ "id": id }),
        );

        Ok(())
    }

    pub async fn rename(&self, id: &str, title: &str) -> Result<()> {
        let mut store = self.store.write().await;
        store.rename(id, title)?;
        drop(store);

        self.cache.invalidate(id).await;

        self.event_bus.emit_data(
            id,
            "title_changed",
            serde_json::json!({ "id": id, "title": title }),
        );

        Ok(())
    }

    /// Add a single message to a conversation's active path and persist.
    pub async fn add_message(&self, id: &str, msg: ChatMessage) -> Result<()> {
        let msg_id = msg.id.clone();

        let mut store = self.store.write().await;
        let mut conv = store
            .get(id)
            .context("failed to load conversation")?
            .context("conversation not found")?;

        conv.active_path.push(msg_id);
        conv.messages.push(msg);
        conv.updated_at = Utc::now();
        store.save(&conv)?;
        drop(store);

        self.cache.insert(conv.id.clone(), conv).await;

        self.event_bus.emit_data(
            id,
            "message_added",
            serde_json::json!({ "id": id }),
        );

        Ok(())
    }

    /// Add multiple messages and path IDs, then persist.
    pub async fn add_messages(
        &self,
        id: &str,
        msgs: Vec<ChatMessage>,
        path_ids: Vec<String>,
    ) -> Result<()> {
        let mut store = self.store.write().await;
        let mut conv = store
            .get(id)
            .context("failed to load conversation")?
            .context("conversation not found")?;

        conv.messages.extend(msgs);
        conv.active_path.extend(path_ids);
        conv.updated_at = Utc::now();
        store.save(&conv)?;
        drop(store);

        self.cache.insert(conv.id.clone(), conv).await;

        self.event_bus.emit_data(
            id,
            "message_added",
            serde_json::json!({ "id": id }),
        );

        Ok(())
    }

    /// Replace the active path for a conversation and persist.
    pub async fn update_path(&self, id: &str, new_path: Vec<String>) -> Result<()> {
        let mut store = self.store.write().await;
        let mut conv = store
            .get(id)
            .context("failed to load conversation")?
            .context("conversation not found")?;

        conv.active_path = new_path;
        conv.updated_at = Utc::now();
        store.save(&conv)?;
        drop(store);

        self.cache.insert(conv.id.clone(), conv).await;

        Ok(())
    }

    /// Update usage stats for a conversation and persist.
    pub async fn update_usage(&self, id: &str, usage: ConversationUsage) -> Result<()> {
        let mut store = self.store.write().await;
        let mut conv = store
            .get(id)
            .context("failed to load conversation")?
            .context("conversation not found")?;

        conv.usage = Some(usage);
        conv.updated_at = Utc::now();
        store.save(&conv)?;
        drop(store);

        self.cache.insert(conv.id.clone(), conv).await;

        Ok(())
    }

    /// Add cost to a conversation's running total.
    pub async fn add_cost(&self, id: &str, cost: f64) -> Result<()> {
        let mut store = self.store.write().await;
        let mut conv = store
            .get(id)
            .context("failed to load conversation")?
            .context("conversation not found")?;

        if let Some(ref mut usage) = conv.usage {
            usage.total_cost += cost;
        } else {
            conv.usage = Some(ConversationUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                context_window: 0,
                total_cost: cost,
            });
        }
        store.save(&conv)?;
        drop(store);

        self.cache.insert(conv.id.clone(), conv).await;

        Ok(())
    }

    // ── Complex mutations (checkout/commit) ──

    /// Get a mutable clone of a conversation for complex modifications.
    /// Caller modifies the clone, then calls `commit()` to save it back.
    pub async fn checkout(&self, id: &str) -> Result<Option<Conversation>> {
        self.get(id).await
    }

    /// Save a modified conversation back to the store, update cache, emit event.
    pub async fn commit(&self, conv: Conversation) -> Result<()> {
        let id = conv.id.clone();

        let mut store = self.store.write().await;
        store.save(&conv)?;
        drop(store);

        self.cache.insert(id.clone(), conv).await;

        self.event_bus.emit_data(
            &id,
            "thread_updated",
            serde_json::json!({ "id": &id }),
        );

        Ok(())
    }
}
