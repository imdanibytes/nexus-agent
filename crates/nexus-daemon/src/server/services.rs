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
use super::sse::AgentEventBridge;

/// Chat execution: conversations, cancellation, events, questions, tasks.
pub struct ChatService {
    pub conversations: RwLock<ConversationStore>,
    pub active_cancel: Mutex<Option<(String, tokio_util::sync::CancellationToken)>>,
    pub event_bridge: AgentEventBridge,
    pub pending_questions: RwLock<PendingQuestionStore>,
    pub task_store: RwLock<TaskStateStore>,
    pub process_manager: Arc<ProcessManager>,
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

