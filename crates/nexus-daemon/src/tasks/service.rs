use std::path::PathBuf;

use tokio::sync::RwLock;

use nexus_core::tasks::{TaskState, TaskStateStore};
use crate::event_bus::EventBus;

/// Encapsulated task state service.
///
/// Wraps `TaskStateStore` with internal locking, method-based access, and
/// EventBus emission on mutations. The agent layer can access the inner
/// store via `store()` during the transition to full encapsulation.
pub struct TaskService {
    inner: RwLock<TaskStateStore>,
    event_bus: EventBus,
}

impl TaskService {
    pub fn new(base_dir: PathBuf, event_bus: EventBus) -> Self {
        Self {
            inner: RwLock::new(TaskStateStore::new(base_dir)),
            event_bus,
        }
    }

    // -- Reads ----------------------------------------------------------------

    /// Get task state for a conversation (lazy-loads from disk).
    pub async fn get(&self, conversation_id: &str) -> Option<TaskState> {
        self.inner.write().await.get(conversation_id)
    }

    /// Get task state or a default empty state (lazy-loads from disk).
    pub async fn get_or_default(&self, conversation_id: &str) -> TaskState {
        self.inner.write().await.get_or_default(conversation_id).clone()
    }

    // -- Writes ---------------------------------------------------------------

    /// Replace task state for a conversation, persist, and emit event.
    pub async fn set(&self, conversation_id: &str, state: TaskState) -> Result<(), String> {
        let mut store = self.inner.write().await;
        *store.get_or_default(conversation_id) = state.clone();
        store.save(conversation_id)?;
        drop(store);

        self.emit_changed(conversation_id, &state);
        Ok(())
    }

    /// Remove task state for a conversation (memory + disk).
    pub async fn remove(&self, conversation_id: &str) {
        self.inner.write().await.remove(conversation_id);
    }

    // -- Agent layer escape hatch (transitional) ------------------------------

    /// Direct access to the inner store for the agent tool dispatch layer.
    ///
    /// This exists so the agent's `TurnServices` can pass `&RwLock<TaskStateStore>`
    /// through to tool handlers without rewriting the entire dispatch chain.
    /// Will be removed when TurnOrchestrator encapsulates the agent layer.
    pub fn store(&self) -> &RwLock<TaskStateStore> {
        &self.inner
    }

    // -- Internal -------------------------------------------------------------

    fn emit_changed(&self, conversation_id: &str, state: &TaskState) {
        self.event_bus.emit_data(
            conversation_id,
            "task_state_changed",
            serde_json::json!({
                "conversationId": conversation_id,
                "plan": state.plan,
                "tasks": state.tasks,
                "mode": state.mode,
            }),
        );
    }
}
