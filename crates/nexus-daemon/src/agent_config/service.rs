use anyhow::Result;
use tokio::sync::RwLock;

use super::store::{AgentStore, AgentUpdate, CreateAgentParams};
use super::types::AgentEntry;
use crate::event_bus::EventBus;

/// Encapsulated agent configuration service.
///
/// Wraps `AgentStore` with internal locking, method-based access, and
/// EventBus emission on mutations. Callers never see the inner store or lock.
pub struct AgentService {
    store: RwLock<AgentStore>,
    event_bus: EventBus,
}

impl AgentService {
    pub fn new(store: AgentStore, event_bus: EventBus) -> Self {
        Self {
            store: RwLock::new(store),
            event_bus,
        }
    }

    // -- Reads ----------------------------------------------------------------

    pub async fn list(&self) -> Vec<AgentEntry> {
        self.store.read().await.list().to_vec()
    }

    pub async fn get(&self, id: &str) -> Option<AgentEntry> {
        self.store.read().await.get(id).cloned()
    }

    pub async fn active_agent_id(&self) -> Option<String> {
        self.store.read().await.active_agent_id().map(|s| s.to_string())
    }

    pub async fn active_agent(&self) -> Option<AgentEntry> {
        let store = self.store.read().await;
        let id = store.active_agent_id()?;
        store.get(id).cloned()
    }

    // -- Writes ---------------------------------------------------------------

    pub async fn create(&self, params: CreateAgentParams) -> Result<AgentEntry> {
        let agent = self.store.write().await.create(params)?;
        self.event_bus.emit_global("agent_created", serde_json::to_value(&agent).unwrap_or_default());
        Ok(agent)
    }

    pub async fn update(&self, id: &str, updates: AgentUpdate) -> Result<Option<AgentEntry>> {
        let result = self.store.write().await.update(id, updates)?;
        if let Some(ref agent) = result {
            self.event_bus.emit_global("agent_updated", serde_json::to_value(agent).unwrap_or_default());
        }
        Ok(result)
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let deleted = self.store.write().await.delete(id)?;
        if deleted {
            self.event_bus.emit_global("agent_deleted", serde_json::json!({ "id": id }));
        }
        Ok(deleted)
    }

    pub async fn set_active(&self, id: Option<String>) -> Result<()> {
        self.store.write().await.set_active(id.clone())?;
        self.event_bus.emit_global("active_agent_changed", serde_json::json!({ "agent_id": id }));
        Ok(())
    }
}
