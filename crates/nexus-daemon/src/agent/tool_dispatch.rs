use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::ask_user::{self, AskUserArgs, PendingQuestion, PendingQuestionStore, UserAnswer};
use crate::config::{FetchConfig, FilesystemConfig};
use crate::fetch;
use crate::filesystem;
use crate::mcp::McpManager;
use crate::tasks;
use crate::tasks::store::TaskStateStore;
use super::events::AgUiEvent;

/// Result of dispatching a single tool call.
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

/// Context passed to a tool handler for a single invocation.
pub struct ToolContext<'a> {
    pub tool_call_id: &'a str,
    pub tool_name: &'a str,
    pub args_json: &'a str,
    pub conversation_id: &'a str,
    pub tx: &'a broadcast::Sender<AgUiEvent>,
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

        let _ = ctx.tx.send(AgUiEvent::Custom {
            thread_id: ctx.conversation_id.to_string(),
            name: "ask_user_pending".to_string(),
            value: serde_json::json!({
                "questionId": question_id,
                "toolCallId": ctx.tool_call_id,
                "question": ask_args.question,
                "type": ask_args.question_type,
                "options": ask_args.options,
                "context": ask_args.context,
                "placeholder": ask_args.placeholder,
            }),
        });

        let _ = ctx.tx.send(AgUiEvent::Custom {
            thread_id: ctx.conversation_id.to_string(),
            name: "activity_update".to_string(),
            value: serde_json::json!({ "activity": "Waiting for your input..." }),
        });

        let (content, is_error) = tokio::select! {
            answer = resp_rx => {
                match answer {
                    Ok(a) => {
                        let _ = ctx.tx.send(AgUiEvent::Custom {
                            thread_id: ctx.conversation_id.to_string(),
                            name: "ask_user_answered".to_string(),
                            value: serde_json::json!({
                                "questionId": question_id,
                                "toolCallId": ctx.tool_call_id,
                                "value": a.value,
                            }),
                        });
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

        ToolResult { content, is_error }
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
            ctx.tool_name, &args, ctx.conversation_id, self.task_store, ctx.tx,
        ).await;
        ToolResult { content, is_error }
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
                };
            }
        };

        // Emit activity update
        let _ = ctx.tx.send(AgUiEvent::Custom {
            thread_id: ctx.conversation_id.to_string(),
            name: "activity_update".to_string(),
            value: serde_json::json!({ "activity": format!("Fetching {}...", args.url) }),
        });

        match fetch::execute_fetch(&args, self.fetch_config).await {
            Ok(content) => ToolResult {
                content,
                is_error: false,
            },
            Err(e) => ToolResult {
                content: e,
                is_error: true,
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
        let _ = ctx.tx.send(AgUiEvent::Custom {
            thread_id: ctx.conversation_id.to_string(),
            name: "activity_update".to_string(),
            value: serde_json::json!({ "activity": format!("{}...", ctx.tool_name) }),
        });

        match filesystem::execute(ctx.tool_name, ctx.args_json, &self.validator) {
            Ok(content) => ToolResult {
                content,
                is_error: false,
            },
            Err(e) => ToolResult {
                content: e,
                is_error: true,
            },
        }
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
        ToolResult { content, is_error }
    }
}
