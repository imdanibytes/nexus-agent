use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use nexus_tools::ask_user::PendingQuestionStore;
use crate::bg_process::ProcessManager;
use crate::mcp::store::McpServerStore;
use crate::mcp::McpManager;
use super::message_queue::MessageQueue;
use super::sse::AgentEventBridge;

/// A turn that is currently active for a conversation.
pub struct ActiveTurn {
    pub run_id: String,
    pub cancel: tokio_util::sync::CancellationToken,
}

/// Turn lifecycle manager: active turn tracking, cancellation, events,
/// pending questions, background processes, and message queue.
///
/// Conversation CRUD → `ThreadService`. Task state → `TaskService`.
pub struct TurnManager {
    active_turns: Mutex<HashMap<String, ActiveTurn>>,
    pub event_bridge: AgentEventBridge,
    pub pending_questions: RwLock<PendingQuestionStore>,
    pub process_manager: Arc<ProcessManager>,
    pub message_queue: Arc<MessageQueue>,
}

impl TurnManager {
    pub fn new(
        event_bridge: AgentEventBridge,
        pending_questions: PendingQuestionStore,
        process_manager: Arc<ProcessManager>,
        message_queue: Arc<MessageQueue>,
    ) -> Self {
        Self {
            active_turns: Mutex::new(HashMap::new()),
            event_bridge,
            pending_questions: RwLock::new(pending_questions),
            process_manager,
            message_queue,
        }
    }

    /// Register a new turn, cancelling any existing turn for this conversation.
    /// Returns the cancellation token and run ID for the new turn.
    pub async fn register_turn(
        &self,
        conversation_id: &str,
    ) -> (tokio_util::sync::CancellationToken, String) {
        let run_id = uuid::Uuid::new_v4().to_string();
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut active = self.active_turns.lock().await;
        if let Some(prev) = active.remove(conversation_id) {
            prev.cancel.cancel();
        }
        active.insert(
            conversation_id.to_string(),
            ActiveTurn {
                run_id: run_id.clone(),
                cancel: cancel.clone(),
            },
        );
        (cancel, run_id)
    }

    /// Cancel the active turn for a conversation, if any.
    /// Returns the cancelled turn's run_id, or None if no turn was active.
    pub async fn cancel_turn(&self, conversation_id: &str) -> Option<String> {
        let mut active = self.active_turns.lock().await;
        if let Some(turn) = active.remove(conversation_id) {
            turn.cancel.cancel();
            Some(turn.run_id)
        } else {
            None
        }
    }

    /// Check if a turn is active for a conversation.
    pub async fn is_active(&self, conversation_id: &str) -> bool {
        self.active_turns.lock().await.contains_key(conversation_id)
    }

    /// Get all active run IDs (for SSE subscriber replay).
    pub async fn active_run_ids(&self) -> Vec<String> {
        self.active_turns
            .lock()
            .await
            .values()
            .map(|t| t.run_id.clone())
            .collect()
    }

    /// Remove the active turn if it matches the given run_id.
    /// Returns true if the turn was removed (i.e., it was still ours).
    pub async fn finish_turn(&self, conversation_id: &str, run_id: &str) -> bool {
        let mut active = self.active_turns.lock().await;
        let is_mine = active
            .get(conversation_id)
            .map(|t| t.run_id == run_id)
            .unwrap_or(false);
        if is_mine {
            active.remove(conversation_id);
        }
        is_mine
    }
}

/// MCP server management.
pub struct McpService {
    pub mcp: RwLock<McpManager>,
    pub configs: RwLock<McpServerStore>,
}
