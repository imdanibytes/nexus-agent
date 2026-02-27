use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use std::sync::Arc;

use crate::ask_user::{self, AskUserArgs, PendingQuestion, PendingQuestionStore, UserAnswer};
use crate::bash;
use crate::bg_process::{ProcessKind, ProcessManager};
use crate::config::{FetchConfig, FilesystemConfig};
use crate::fetch;
use crate::filesystem;
use crate::mcp::McpManager;
use crate::tasks;
use crate::tasks::store::TaskStateStore;
use super::emitter::TurnEmitter;

/// Result of dispatching a single tool call.
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
    /// Optional LSP diagnostics to inject as a separate user message
    /// (not part of the tool result content).
    pub lsp_diagnostics: Option<String>,
}

/// Context passed to a tool handler for a single invocation.
pub struct ToolContext<'a> {
    pub tool_call_id: &'a str,
    pub tool_name: &'a str,
    pub args_json: &'a str,
    pub conversation_id: &'a str,
    pub emitter: &'a TurnEmitter,
    pub cancel: &'a CancellationToken,
}

/// Strategy trait for dispatching tool calls.
///
/// Implementations handle a specific category of tools (ask_user, builtins,
/// MCP, etc.). The agent loop constructs a handler list once per turn and
/// `dispatch_tool_call` iterates it for each tool invocation.
#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn can_handle(&self, tool_name: &str) -> bool;
    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult;
}

/// Dispatch a tool call by iterating the handler chain.
pub async fn dispatch_tool_call(
    handlers: &[&dyn ToolHandler],
    ctx: ToolContext<'_>,
) -> ToolResult {
    for handler in handlers {
        if handler.can_handle(ctx.tool_name) {
            return handler.handle(&ctx).await;
        }
    }
    ToolResult {
        content: format!("Unknown tool: {}", ctx.tool_name),
        is_error: true,
        lsp_diagnostics: None,
    }
}

// ── AskUserHandler ──

pub struct AskUserHandler<'a> {
    pub pending_questions: &'a tokio::sync::RwLock<PendingQuestionStore>,
}

#[async_trait]
impl ToolHandler for AskUserHandler<'_> {
    fn can_handle(&self, tool_name: &str) -> bool {
        ask_user::is_ask_user(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        let ask_args: AskUserArgs = serde_json::from_str(ctx.args_json)
            .unwrap_or_else(|_| AskUserArgs {
                question: "Continue?".to_string(),
                question_type: ask_user::QuestionType::Confirm,
                options: None,
                context: None,
                placeholder: None,
            });

        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<UserAnswer>();
        let question_id = Uuid::new_v4().to_string();

        {
            let mut pq = self.pending_questions.write().await;
            pq.insert(PendingQuestion {
                id: question_id.clone(),
                conversation_id: ctx.conversation_id.to_string(),
                tool_call_id: ctx.tool_call_id.to_string(),
                args: ask_args.clone(),
                created_at: chrono::Utc::now(),
                response_tx: resp_tx,
            });
        }

        ctx.emitter.custom("ask_user_pending", serde_json::json!({
            "questionId": question_id,
            "toolCallId": ctx.tool_call_id,
            "question": ask_args.question,
            "type": ask_args.question_type,
            "options": ask_args.options,
            "context": ask_args.context,
            "placeholder": ask_args.placeholder,
        }));

        ctx.emitter.activity("Waiting for your input...");

        let (content, is_error) = tokio::select! {
            answer = resp_rx => {
                match answer {
                    Ok(a) => {
                        ctx.emitter.custom("ask_user_answered", serde_json::json!({
                            "questionId": question_id,
                            "toolCallId": ctx.tool_call_id,
                            "value": a.value,
                        }));
                        if a.dismissed {
                            ("User dismissed the question.".to_string(), true)
                        } else {
                            (serde_json::json!({ "answer": a.value }).to_string(), false)
                        }
                    }
                    Err(_) => {
                        ("Question cancelled (channel closed).".to_string(), true)
                    }
                }
            }
            _ = ctx.cancel.cancelled() => {
                let mut pq = self.pending_questions.write().await;
                pq.remove(&question_id);
                ("Cancelled".to_string(), true)
            }
        };

        ToolResult { content, is_error, lsp_diagnostics: None }
    }
}

// ── TaskToolHandler ──

pub struct TaskToolHandler<'a> {
    pub task_store: &'a tokio::sync::RwLock<TaskStateStore>,
}

#[async_trait]
impl ToolHandler for TaskToolHandler<'_> {
    fn can_handle(&self, tool_name: &str) -> bool {
        tasks::tools::is_builtin(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        let args: serde_json::Value = serde_json::from_str(ctx.args_json)
            .unwrap_or_else(|_| serde_json::json!({}));
        let (content, is_error) = tasks::tools::handle_builtin(
            ctx.tool_name, &args, ctx.conversation_id, self.task_store, ctx.emitter,
        ).await;
        ToolResult { content, is_error, lsp_diagnostics: None }
    }
}

// ── FetchHandler ──

pub struct FetchHandler<'a> {
    pub fetch_config: &'a FetchConfig,
}

#[async_trait]
impl ToolHandler for FetchHandler<'_> {
    fn can_handle(&self, tool_name: &str) -> bool {
        fetch::is_fetch(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        let args: fetch::FetchArgs = match serde_json::from_str(ctx.args_json) {
            Ok(a) => a,
            Err(e) => {
                return ToolResult {
                    content: format!("Invalid fetch arguments: {e}"),
                    is_error: true,
                    lsp_diagnostics: None,
                };
            }
        };

        // Emit activity update
        ctx.emitter.activity(format!("Fetching {}...", args.url));

        match fetch::execute_fetch(&args, self.fetch_config).await {
            Ok(content) => ToolResult {
                content,
                is_error: false,
            lsp_diagnostics: None,
            },
            Err(e) => ToolResult {
                content: e,
                is_error: true,
            lsp_diagnostics: None,
            },
        }
    }
}

// ── FilesystemHandler ──

pub struct FilesystemHandler {
    pub validator: filesystem::PathValidator,
}

impl FilesystemHandler {
    pub fn new(config: &FilesystemConfig) -> Self {
        Self {
            validator: filesystem::PathValidator::new(&config.allowed_directories),
        }
    }
}

#[async_trait]
impl ToolHandler for FilesystemHandler {
    fn can_handle(&self, tool_name: &str) -> bool {
        filesystem::is_filesystem_tool(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        // Activity update
        ctx.emitter.activity(format!("{}...", ctx.tool_name));

        match filesystem::execute(ctx.tool_name, ctx.args_json, &self.validator) {
            Ok(content) => ToolResult {
                content,
                is_error: false,
            lsp_diagnostics: None,
            },
            Err(e) => ToolResult {
                content: e,
                is_error: true,
            lsp_diagnostics: None,
            },
        }
    }
}

// ── BashHandler ──

pub struct BashHandler {
    pub working_dir: Option<String>,
    pub process_manager: Arc<ProcessManager>,
}

#[async_trait]
impl ToolHandler for BashHandler {
    fn can_handle(&self, tool_name: &str) -> bool {
        bash::is_bash(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        let args: serde_json::Value = serde_json::from_str(ctx.args_json)
            .unwrap_or_else(|_| serde_json::json!({}));

        let command = match args.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => {
                return ToolResult {
                    content: "Missing required field: 'command'".to_string(),
                    is_error: true,
                lsp_diagnostics: None,
                };
            }
        };

        let run_in_background = args
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if run_in_background {
            return self.spawn_background_bash(ctx, command).await;
        }

        let timeout_ms = args.get("timeout_ms").and_then(|v| v.as_u64());

        ctx.emitter.activity(format!("Running: {}",
            if command.len() > 60 { &command[..60] } else { command }
        ));

        let (content, is_error) = bash::execute(
            command,
            timeout_ms,
            self.working_dir.as_deref(),
        )
        .await;

        ToolResult { content, is_error, lsp_diagnostics: None }
    }
}

impl BashHandler {
    async fn spawn_background_bash(&self, ctx: &ToolContext<'_>, command: &str) -> ToolResult {
        // Extract label from the description field the model fills out
        let label = serde_json::from_str::<serde_json::Value>(ctx.args_json)
            .ok()
            .and_then(|v| v.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| {
                let truncated = if command.len() > 60 { &command[..60] } else { command };
                format!("Running: {}", truncated)
            });

        let spawn_result = match self.process_manager.spawn(
            ctx.conversation_id,
            label,
            command.to_string(),
            ProcessKind::Bash,
        ).await {
            Ok(r) => r,
            Err(e) => return ToolResult { content: e, is_error: true, lsp_diagnostics: None },
        };

        let process_id = spawn_result.process_id.clone();
        let cancel_token = spawn_result.cancel_token;
        let output_path = spawn_result.output_path;
        let command_owned = command.to_string();
        let working_dir = self.working_dir.clone();
        let pm = Arc::clone(&self.process_manager);

        tokio::spawn(async move {
            let result = tokio::select! {
                result = bash::execute(&command_owned, None, working_dir.as_deref()) => result,
                _ = cancel_token.cancelled() => {
                    ("Cancelled".to_string(), true)
                }
            };

            let (output, is_error) = result;
            let exit_code = if is_error { Some(1) } else { Some(0) };

            // Write output to file
            let _ = tokio::fs::write(&output_path, &output).await;

            pm.complete(&process_id, exit_code, is_error).await;
        });

        ToolResult {
            content: serde_json::json!({
                "process_id": spawn_result.process_id,
                "status": "running",
                "message": "Process started in background. You will be notified when it completes. Use process_output to read output."
            }).to_string(),
            is_error: false,
            lsp_diagnostics: None,
        }
    }
}

// ── ResourceToolHandler ──

pub struct ResourceToolHandler<'a> {
    pub mcp: &'a McpManager,
}

#[async_trait]
impl ToolHandler for ResourceToolHandler<'_> {
    fn can_handle(&self, tool_name: &str) -> bool {
        crate::mcp_resources::is_resource_tool(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        ctx.emitter.activity(format!("{}...", ctx.tool_name));
        let (content, is_error) = crate::mcp_resources::execute(
            ctx.tool_name,
            ctx.args_json,
            self.mcp,
        )
        .await;
        ToolResult { content, is_error, lsp_diagnostics: None }
    }
}

// ── ControlPlaneHandler ──

pub struct ControlPlaneHandler {
    pub deps: Arc<crate::control_plane::ControlPlaneDeps>,
}

#[async_trait]
impl ToolHandler for ControlPlaneHandler {
    fn can_handle(&self, tool_name: &str) -> bool {
        crate::control_plane::is_control_plane(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        ctx.emitter.activity(format!("{}...", ctx.tool_name));
        let (content, is_error) = crate::control_plane::execute(
            ctx.tool_name,
            ctx.args_json,
            ctx.conversation_id,
            &self.deps,
        )
        .await;
        ToolResult { content, is_error, lsp_diagnostics: None }
    }
}

// ── McpToolHandler ──

pub struct McpToolHandler<'a> {
    pub mcp: &'a McpManager,
}

#[async_trait]
impl ToolHandler for McpToolHandler<'_> {
    fn can_handle(&self, _tool_name: &str) -> bool {
        true // MCP is the fallback — handles everything not matched above
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        let (content, is_error) = self.mcp.call_tool(ctx.tool_name, ctx.args_json).await;
        ToolResult { content, is_error, lsp_diagnostics: None }
    }
}
