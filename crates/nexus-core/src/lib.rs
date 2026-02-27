use std::sync::Arc;

use async_trait::async_trait;

// ── Injected message ──

/// An ephemeral message injected by a module alongside a tool result.
/// Not persisted to conversation history — only sent to the API for context.
#[derive(Debug, Clone)]
pub struct InjectedMessage {
    pub text: String,
}

// ── Tool result ──

/// Result of dispatching a single tool call.
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
    /// Ephemeral messages to inject alongside this result.
    pub injected_messages: Vec<InjectedMessage>,
}

impl ToolResult {
    pub fn success(content: String) -> Self {
        Self {
            content,
            is_error: false,
            injected_messages: Vec::new(),
        }
    }

    pub fn error(content: String) -> Self {
        Self {
            content,
            is_error: true,
            injected_messages: Vec::new(),
        }
    }
}

// ── Module-owned types (decoupled from provider-specific types) ──

/// Tool definition provided by a module.
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Why the agent stopped responding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EndTurn => write!(f, "end_turn"),
            Self::MaxTokens => write!(f, "max_tokens"),
            Self::StopSequence => write!(f, "stop_sequence"),
            Self::ToolUse => write!(f, "tool_use"),
        }
    }
}

// ── Event types ──

/// PreToolUse — fires before tool execution.
pub struct PreToolUseEvent<'a> {
    pub tool_name: &'a str,
    pub tool_input: &'a serde_json::Value,
    pub conversation_id: &'a str,
}

/// Decision returned by `pre_tool_use`.
pub enum PreToolUseDecision {
    /// Allow tool execution with original args.
    Allow,
    /// Block execution; reason is fed back to the LLM as an error.
    Deny(String),
    /// Replace tool args before execution.
    ModifyArgs(serde_json::Value),
}

/// PostToolUse — fires after successful tool execution.
pub struct PostToolUseEvent<'a> {
    pub tool_name: &'a str,
    pub tool_input: &'a serde_json::Value,
    pub result: &'a mut ToolResult,
    pub conversation_id: &'a str,
}

/// PostToolUseFailure — fires after failed tool execution.
pub struct PostToolUseFailureEvent<'a> {
    pub tool_name: &'a str,
    pub tool_input: &'a serde_json::Value,
    pub error: &'a str,
    pub conversation_id: &'a str,
}

/// UserPromptSubmit — fires before message enters agent loop.
pub struct UserPromptSubmitEvent<'a> {
    pub prompt: &'a str,
    pub conversation_id: &'a str,
    /// Modules can push additional context here (injected as ephemeral user messages).
    pub additional_context: &'a mut Vec<String>,
}

/// TurnStart — fires when a turn begins (round 0).
pub struct TurnStartEvent<'a> {
    pub conversation_id: &'a str,
    pub run_id: &'a str,
    pub depth: u32,
}

/// Stop — fires when the agent finishes responding.
pub struct StopEvent<'a> {
    pub conversation_id: &'a str,
    pub round_count: usize,
    pub stop_reason: &'a StopReason,
}

/// Decision returned by `stop`.
pub enum StopDecision {
    /// Normal stop.
    Stop,
    /// Force another round with a reason injected as user context.
    Continue(String),
}

/// TurnEnd — fires after turn results are persisted.
pub struct TurnEndEvent<'a> {
    pub conversation_id: &'a str,
    pub run_id: &'a str,
    pub round_count: usize,
    pub turn_cost: f64,
    pub error: Option<&'a str>,
}

/// PreCompact — fires before context compaction.
pub struct PreCompactEvent<'a> {
    pub conversation_id: &'a str,
    pub estimated_tokens: u32,
    pub context_window: u32,
    pub layer: CompactionLayer,
}

/// Which compaction layer is about to run.
pub enum CompactionLayer {
    Prune,
    Summarize,
}

/// SubagentStart — fires when a sub-agent spawns.
pub struct SubagentStartEvent<'a> {
    pub parent_conversation_id: &'a str,
    pub depth: u32,
}

/// SubagentStop — fires when a sub-agent finishes.
pub struct SubagentStopEvent<'a> {
    pub parent_conversation_id: &'a str,
    pub depth: u32,
    pub error: Option<&'a str>,
}

/// TaskCompleted — fires when a task is marked complete.
pub struct TaskCompletedEvent<'a> {
    pub conversation_id: &'a str,
    pub task_id: &'a str,
    pub task_title: &'a str,
}

/// ConfigChange — fires when a config file changes.
pub struct ConfigChangeEvent<'a> {
    pub source: &'a str,
    pub file_path: &'a str,
}

/// Notification — fires when the daemon emits an event.
pub struct NotificationEvent<'a> {
    pub event_type: &'a str,
    pub data: &'a serde_json::Value,
}

/// PermissionRequest — fires when a tool needs permission (future).
pub struct PermissionRequestEvent<'a> {
    pub tool_name: &'a str,
    pub tool_input: &'a serde_json::Value,
    pub conversation_id: &'a str,
}

/// Decision returned by `permission_request`.
pub enum PermissionDecision {
    /// No opinion — let normal flow decide.
    Pass,
    Allow,
    Deny(String),
}

// ── Doctor / health ──

/// Health report from a module.
pub struct DoctorReport {
    pub module: String,
    pub status: DoctorStatus,
    pub checks: Vec<DoctorCheck>,
}

pub enum DoctorStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Disabled,
}

pub struct DoctorCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

// ── DaemonModule trait ──

/// Extension point for daemon functionality.
///
/// One trait, one method per hook, all defaulting to no-op.
/// Modules override only the hooks they need.
#[async_trait]
pub trait DaemonModule: Send + Sync {
    /// Unique name for this module (for logging, doctor reports).
    fn name(&self) -> &str;

    // ── Tool lifecycle ──

    /// Provide tool definitions for the agent. Called each turn.
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![]
    }

    /// Before a tool call executes. Can deny, modify args, or allow.
    async fn pre_tool_use(&self, _event: &PreToolUseEvent<'_>) -> PreToolUseDecision {
        PreToolUseDecision::Allow
    }

    /// After a tool call succeeds. Can decorate the result or observe.
    async fn post_tool_use(&self, _event: &mut PostToolUseEvent<'_>) {}

    /// After a tool call fails.
    async fn post_tool_use_failure(&self, _event: &PostToolUseFailureEvent<'_>) {}

    // ── Message lifecycle ──

    /// User sends a message, before it enters the agent loop.
    async fn user_prompt_submit(&self, _event: &mut UserPromptSubmitEvent<'_>) {}

    // ── Turn lifecycle ──

    /// Turn begins (first round about to start).
    async fn turn_start(&self, _event: &TurnStartEvent<'_>) {}

    /// Agent finished responding. Return Continue to force another round.
    async fn stop(&self, _event: &StopEvent<'_>) -> StopDecision {
        StopDecision::Stop
    }

    /// Turn fully complete (results persisted, cleanup done).
    async fn turn_end(&self, _event: &TurnEndEvent<'_>) {}

    // ── Compaction ──

    /// Before context compaction. Save any state that would be lost.
    async fn pre_compact(&self, _event: &PreCompactEvent<'_>) {}

    // ── Sub-agents ──

    async fn subagent_start(&self, _event: &SubagentStartEvent<'_>) {}
    async fn subagent_stop(&self, _event: &SubagentStopEvent<'_>) {}

    // ── Tasks ──

    async fn task_completed(&self, _event: &TaskCompletedEvent<'_>) {}

    // ── Config ──

    async fn config_change(&self, _event: &ConfigChangeEvent<'_>) {}

    // ── Notifications ──

    async fn notification(&self, _event: &NotificationEvent<'_>) {}

    // ── Permissions (future) ──

    async fn permission_request(
        &self,
        _event: &PermissionRequestEvent<'_>,
    ) -> PermissionDecision {
        PermissionDecision::Pass
    }

    // ── Daemon lifecycle ──

    async fn on_startup(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn on_shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }

    // ── Health ──

    async fn doctor(&self) -> DoctorReport;
}

// ── ModuleRegistry ──

/// Registry of daemon modules. Iterates modules for each hook.
#[derive(Default)]
pub struct ModuleRegistry {
    modules: Vec<Arc<dyn DaemonModule>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, module: Arc<dyn DaemonModule>) {
        tracing::info!(module = module.name(), "Registered daemon module");
        self.modules.push(module);
    }

    pub fn modules(&self) -> &[Arc<dyn DaemonModule>] {
        &self.modules
    }

    // ── Tool lifecycle ──

    /// Collect tool definitions from all modules.
    pub fn collect_tools(&self) -> Vec<ToolDefinition> {
        self.modules
            .iter()
            .flat_map(|m| m.tool_definitions())
            .collect()
    }

    /// Fire PreToolUse across modules. First non-Allow wins.
    pub async fn fire_pre_tool_use(&self, event: &PreToolUseEvent<'_>) -> PreToolUseDecision {
        for module in &self.modules {
            let decision = module.pre_tool_use(event).await;
            match decision {
                PreToolUseDecision::Allow => continue,
                other => return other,
            }
        }
        PreToolUseDecision::Allow
    }

    /// Fire PostToolUse across all modules.
    pub async fn fire_post_tool_use(&self, event: &mut PostToolUseEvent<'_>) {
        for module in &self.modules {
            module.post_tool_use(event).await;
        }
    }

    /// Fire PostToolUseFailure across all modules.
    pub async fn fire_post_tool_use_failure(&self, event: &PostToolUseFailureEvent<'_>) {
        for module in &self.modules {
            module.post_tool_use_failure(event).await;
        }
    }

    // ── Message lifecycle ──

    pub async fn fire_user_prompt_submit(&self, event: &mut UserPromptSubmitEvent<'_>) {
        for module in &self.modules {
            module.user_prompt_submit(event).await;
        }
    }

    // ── Turn lifecycle ──

    pub async fn fire_turn_start(&self, event: &TurnStartEvent<'_>) {
        for module in &self.modules {
            module.turn_start(event).await;
        }
    }

    /// Fire Stop across modules. First Continue wins.
    pub async fn fire_stop(&self, event: &StopEvent<'_>) -> StopDecision {
        for module in &self.modules {
            let decision = module.stop(event).await;
            if matches!(decision, StopDecision::Continue(_)) {
                return decision;
            }
        }
        StopDecision::Stop
    }

    pub async fn fire_turn_end(&self, event: &TurnEndEvent<'_>) {
        for module in &self.modules {
            module.turn_end(event).await;
        }
    }

    // ── Compaction ──

    pub async fn fire_pre_compact(&self, event: &PreCompactEvent<'_>) {
        for module in &self.modules {
            module.pre_compact(event).await;
        }
    }

    // ── Sub-agents ──

    pub async fn fire_subagent_start(&self, event: &SubagentStartEvent<'_>) {
        for module in &self.modules {
            module.subagent_start(event).await;
        }
    }

    pub async fn fire_subagent_stop(&self, event: &SubagentStopEvent<'_>) {
        for module in &self.modules {
            module.subagent_stop(event).await;
        }
    }

    // ── Tasks ──

    pub async fn fire_task_completed(&self, event: &TaskCompletedEvent<'_>) {
        for module in &self.modules {
            module.task_completed(event).await;
        }
    }

    // ── Config ──

    pub async fn fire_config_change(&self, event: &ConfigChangeEvent<'_>) {
        for module in &self.modules {
            module.config_change(event).await;
        }
    }

    // ── Notifications ──

    pub async fn fire_notification(&self, event: &NotificationEvent<'_>) {
        for module in &self.modules {
            module.notification(event).await;
        }
    }

    // ── Permissions ──

    /// Fire PermissionRequest across modules. First non-Pass wins.
    pub async fn fire_permission_request(
        &self,
        event: &PermissionRequestEvent<'_>,
    ) -> PermissionDecision {
        for module in &self.modules {
            let decision = module.permission_request(event).await;
            match decision {
                PermissionDecision::Pass => continue,
                other => return other,
            }
        }
        PermissionDecision::Pass
    }

    // ── Daemon lifecycle ──

    pub async fn startup(&self) -> anyhow::Result<()> {
        for module in &self.modules {
            module.on_startup().await?;
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        for module in &self.modules {
            if let Err(e) = module.on_shutdown().await {
                tracing::warn!(module = module.name(), error = %e, "Module shutdown error");
            }
        }
    }
}
