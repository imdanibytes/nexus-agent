pub mod emitter;
pub mod events;
pub mod run;
pub mod sub_agent;
pub mod tool_dispatch;

use std::sync::Arc;

use crate::ask_user::PendingQuestionStore;
use crate::bg_process::ProcessManager;
use crate::config::{FetchConfig, FilesystemConfig};
use crate::mcp::McpManager;
use crate::provider::InferenceProvider;
use crate::tasks::store::TaskStateStore;

pub use run::run_agent_turn;

/// A typed timing span for turn profiling.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingSpan {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Result of a completed agent turn: new messages + timing spans + usage.
/// Always contains partial results — even when the turn ended with an error,
/// messages from completed rounds are included so they can be persisted.
pub struct AgentTurnResult {
    pub messages: Vec<crate::anthropic::types::Message>,
    pub timing_spans: Vec<TimingSpan>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub context_window: u32,
    /// Cost incurred during this turn only (USD).
    pub turn_cost: f64,
    /// If the turn ended with an error, this contains the error message.
    pub error: Option<String>,
    /// Structured error details (serialized ProviderError) for the frontend.
    #[allow(dead_code)] // read by the caller that constructs AgentTurnResult
    pub error_details: Option<serde_json::Value>,
}

/// Inference configuration for a single turn.
pub struct InferenceConfig<'a> {
    pub provider: &'a dyn InferenceProvider,
    pub model: &'a str,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub thinking_budget: Option<u32>,
    pub system_prompt: Option<String>,
    pub state_update: Option<String>,
}

/// Conversation context for a single turn.
pub struct TurnContext {
    pub conversation_id: String,
    pub messages: Vec<crate::anthropic::types::Message>,
    pub tools: Vec<crate::anthropic::types::Tool>,
    pub prior_cost: f64,
    pub depth: u32,
}

/// Shared service references for tool dispatch.
pub struct TurnServices<'a> {
    pub mcp: &'a McpManager,
    pub fetch_config: &'a FetchConfig,
    pub filesystem_config: &'a FilesystemConfig,
    pub task_store: &'a tokio::sync::RwLock<TaskStateStore>,
    pub pending_questions: &'a tokio::sync::RwLock<PendingQuestionStore>,
    pub process_manager: Option<Arc<ProcessManager>>,
    pub bg_sub_agent_deps: Option<Arc<sub_agent::BgSubAgentDeps>>,
    pub control_plane: Option<Arc<crate::control_plane::ControlPlaneDeps>>,
    /// LSP service for diagnostics decoration on file operations.
    pub lsp: Option<Arc<crate::lsp::LspService>>,
    /// Project paths from the conversation's active workspace (for LSP scoping).
    pub workspace_project_paths: Vec<String>,
}

pub fn context_window_for_model(model: &str) -> u32 {
    crate::pricing::context_window(model)
}
