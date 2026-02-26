use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::agent_config::AgentStore;
use crate::ask_user::PendingQuestionStore;
use crate::bg_process::ProcessManager;
use crate::conversation::ConversationStore;
use crate::mcp::store::McpServerStore;
use crate::mcp::McpManager;
use crate::provider::{ProviderFactory, ProviderStore};
use crate::tasks::store::TaskStateStore;
use super::message_queue::MessageQueue;
use super::sse::AgentEventBridge;

/// A turn that is currently active for a conversation.
pub struct ActiveTurn {
    pub run_id: String,
    pub cancel: tokio_util::sync::CancellationToken,
}

/// Chat execution: conversations, cancellation, events, questions, tasks.
pub struct ChatService {
    pub conversations: RwLock<ConversationStore>,
    pub active_turns: Mutex<HashMap<String, ActiveTurn>>,
    pub event_bridge: AgentEventBridge,
    pub pending_questions: RwLock<PendingQuestionStore>,
    pub task_store: RwLock<TaskStateStore>,
    pub process_manager: Arc<ProcessManager>,
    pub message_queue: Arc<MessageQueue>,
}

/// Agent + provider configuration and client creation.
pub struct AgentService {
    pub agents: RwLock<AgentStore>,
    pub providers: RwLock<ProviderStore>,
    pub factory: Arc<ProviderFactory>,
}

/// MCP server management.
pub struct McpService {
    pub mcp: RwLock<McpManager>,
    pub configs: RwLock<McpServerStore>,
}

