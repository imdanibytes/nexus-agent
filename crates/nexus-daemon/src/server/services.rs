use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::ask_user::PendingQuestionStore;
use crate::bg_process::ProcessManager;
use crate::mcp::store::McpServerStore;
use crate::mcp::McpManager;
use crate::tasks::store::TaskStateStore;
use super::message_queue::MessageQueue;
use super::sse::AgentEventBridge;

/// A turn that is currently active for a conversation.
pub struct ActiveTurn {
    pub run_id: String,
    pub cancel: tokio_util::sync::CancellationToken,
}

/// Chat execution: cancellation, events, questions, tasks, processes.
///
/// Conversation CRUD has moved to `ThreadService` on `AppState`.
pub struct ChatService {
    pub active_turns: Mutex<HashMap<String, ActiveTurn>>,
    pub event_bridge: AgentEventBridge,
    pub pending_questions: RwLock<PendingQuestionStore>,
    pub task_store: RwLock<TaskStateStore>,
    pub process_manager: Arc<ProcessManager>,
    pub message_queue: Arc<MessageQueue>,
}

/// MCP server management.
pub struct McpService {
    pub mcp: RwLock<McpManager>,
    pub configs: RwLock<McpServerStore>,
}
