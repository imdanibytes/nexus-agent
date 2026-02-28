use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;

use nexus_provider::types::{ContentBlock, Message, Role, Tool};
use nexus_core::bg_process::ProcessKind;
use crate::config::{FetchConfig, FilesystemConfig};
use nexus_provider::InferenceProvider;
use crate::server::services::{TurnManager, McpService};

use super::emitter::TurnEmitter;
use super::tool_dispatch::{ToolContext, ToolHandler, ToolResult};

const SUB_AGENT_TOOL_NAME: &str = "sub_agent";

/// Check if a tool name is the sub_agent built-in.
pub fn is_sub_agent(tool_name: &str) -> bool {
    tool_name == SUB_AGENT_TOOL_NAME
}

/// Return the Anthropic Tool definition for sub_agent.
pub fn tool_definition() -> Tool {
    Tool {
        name: SUB_AGENT_TOOL_NAME.into(),
        description:
            "Spawn a context-isolated sub-agent to handle a focused subtask. The sub-agent \
             runs its own turn loop with a custom system prompt and filtered tools, then \
             returns its output as a tool result. Use this to delegate research, planning, \
             or execution to an isolated context without polluting the main conversation."
                .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The prompt/instructions for the sub-agent"
                },
                "agent_type": {
                    "type": "string",
                    "enum": ["explore", "plan", "execute", "custom"],
                    "default": "explore",
                    "description": "Preset agent type: explore (read-only research), plan (analyze and design), execute (implement changes), custom (user-provided system prompt)"
                },
                "system_prompt": {
                    "type": "string",
                    "description": "Custom system prompt (only used when agent_type is 'custom')"
                },
                "context": {
                    "type": "string",
                    "enum": ["fresh", "branched"],
                    "default": "fresh",
                    "description": "Context mode: fresh (sub-agent gets only the task prompt) or branched (inherits parent conversation history + task appended)"
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "Run the sub-agent in the background. Returns immediately with a process ID. \
                        Use process_output to read output, process_status to check status, \
                        and process_stop to cancel. You will be notified when the sub-agent completes."
                }
            },
            "required": ["task"]
        }),
    }
}

// ── Agent type presets ──

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentType {
    #[default]
    Explore,
    Plan,
    Execute,
    Custom,
}

impl std::fmt::Display for SubAgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explore => write!(f, "explore"),
            Self::Plan => write!(f, "plan"),
            Self::Execute => write!(f, "execute"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    #[default]
    Fresh,
    Branched,
}

/// Resolved sub-agent configuration from tool args + presets.
#[derive(Debug)]
struct SubAgentConfig {
    agent_type: SubAgentType,
    system_prompt: String,
    context_mode: ContextMode,
    task: String,
    /// Tool names to include (empty = all except sub_agent).
    include_task_tools: bool,
}

fn preset_system_prompt(agent_type: SubAgentType, custom: Option<&str>) -> String {
    match agent_type {
        SubAgentType::Explore => {
            "You are a research agent. Read files, search code, analyze patterns, and report \
             findings. Do NOT modify anything — no file writes, no edits, no code execution. \
             Be thorough and specific: include file paths, line numbers, and relevant code \
             snippets in your findings."
                .to_string()
        }
        SubAgentType::Plan => {
            "You are a planning agent. Analyze requirements, design approaches, and identify \
             files to modify. Consider trade-offs, edge cases, and potential issues. Produce \
             a clear, actionable plan. You may use task tools to structure the plan."
                .to_string()
        }
        SubAgentType::Execute => {
            "You are an execution agent. Implement the described task precisely. Write clean, \
             correct code. Test your changes if possible. Report what you changed and any \
             issues encountered."
                .to_string()
        }
        SubAgentType::Custom => custom.unwrap_or("You are a helpful assistant.").to_string(),
    }
}

impl SubAgentConfig {
    fn from_args(args: &serde_json::Value) -> Result<Self, String> {
        let task = args["task"]
            .as_str()
            .ok_or("Missing required field: task")?
            .to_string();

        let agent_type: SubAgentType = args
            .get("agent_type")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let context_mode: ContextMode = args
            .get("context")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let custom_prompt = args["system_prompt"].as_str();
        if agent_type == SubAgentType::Custom && custom_prompt.is_none() {
            return Err(
                "system_prompt is required when agent_type is 'custom'".to_string()
            );
        }

        let system_prompt = preset_system_prompt(agent_type, custom_prompt);

        let include_task_tools = matches!(
            agent_type,
            SubAgentType::Plan | SubAgentType::Execute | SubAgentType::Custom
        );

        Ok(Self {
            agent_type,
            system_prompt,
            context_mode,
            task,
            include_task_tools,
        })
    }
}

/// Filter tools for the sub-agent based on its type.
/// Always excludes `sub_agent` to prevent recursive nesting.
fn filter_tools_for_sub_agent(all_tools: &[Tool], config: &SubAgentConfig) -> Vec<Tool> {
    all_tools
        .iter()
        .filter(|t| {
            // Never give sub-agents the sub_agent tool
            if is_sub_agent(&t.name) {
                return false;
            }

            // Task tools only for plan/execute/custom
            if crate::tasks::tools::is_builtin(&t.name) {
                return config.include_task_tools;
            }

            // ask_user is always available
            // MCP tools are always available
            true
        })
        .cloned()
        .collect()
}

/// Extract text content from sub-agent response messages.
fn extract_text_from_messages(messages: &[Message]) -> String {
    let mut parts = Vec::new();
    for msg in messages {
        if msg.role != Role::Assistant {
            continue;
        }
        for block in &msg.content {
            if let ContentBlock::Text { text } = block {
                if !text.trim().is_empty() {
                    parts.push(text.trim().to_string());
                }
            }
        }
    }
    if parts.is_empty() {
        "(Sub-agent produced no text output)".to_string()
    } else {
        parts.join("\n\n")
    }
}

// ── Background dispatch deps ──

/// Owned dependencies for spawning a sub-agent in a background tokio task.
/// Constructed in `spawn_agent_turn` (turn.rs) where Arc<AppState> is available.
pub struct BgSubAgentDeps {
    pub provider: Arc<dyn InferenceProvider>,
    pub turns: Arc<TurnManager>,
    pub tasks: Arc<crate::tasks::TaskService>,
    pub mcp: Arc<McpService>,
    pub fetch_config: FetchConfig,
    pub filesystem_config: FilesystemConfig,
    pub modules: Arc<crate::module::ModuleRegistry>,
}

// ── Handler ──

pub struct SubAgentHandler<'a> {
    pub inference: &'a super::InferenceConfig<'a>,
    pub services: &'a super::TurnServices<'a>,
    pub parent_messages: &'a [Message],
    pub parent_tools: &'a [Tool],
    /// Cumulative cost so far (prior_cost + parent turn_cost) so sub-agent
    /// usage_update events show the correct running total.
    pub cumulative_cost: f64,
}

#[async_trait]
impl ToolHandler for SubAgentHandler<'_> {
    fn can_handle(&self, tool_name: &str) -> bool {
        is_sub_agent(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        let args: serde_json::Value = serde_json::from_str(ctx.args_json)
            .unwrap_or_else(|_| serde_json::json!({}));

        let config = match SubAgentConfig::from_args(&args) {
            Ok(c) => c,
            Err(e) => {
                return ToolResult {
                    content: serde_json::json!({ "error": e }).to_string(),
                    is_error: true,
                    injected_messages: Vec::new(),
                };
            }
        };

        let run_in_background = args
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if run_in_background {
            return self.spawn_background_sub_agent(ctx, &config).await;
        }

        // Emit sub_agent_start event
        ctx.emitter.sub_agent_start(
            &config.agent_type.to_string(),
            &config.task,
            if config.context_mode == ContextMode::Branched { "branched" } else { "fresh" },
        );

        // Build messages based on context mode
        let messages = match config.context_mode {
            ContextMode::Fresh => vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: config.task.clone(),
                }],
            }],
            ContextMode::Branched => {
                let mut msgs = self.parent_messages.to_vec();
                msgs.push(Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: config.task.clone(),
                    }],
                });
                msgs
            }
        };

        // Filter tools for the sub-agent
        let tools = filter_tools_for_sub_agent(self.parent_tools, &config);

        tracing::info!(
            agent_type = %config.agent_type,
            context = ?config.context_mode,
            tool_count = tools.len(),
            "Spawning sub-agent"
        );

        // Run the sub-agent turn (depth=1 prevents recursive sub-agent spawning)
        let sub_inference = super::InferenceConfig {
            provider: self.inference.provider,
            model: self.inference.model,
            max_tokens: self.inference.max_tokens,
            temperature: self.inference.temperature,
            thinking_budget: None,
            system_prompt: Some(config.system_prompt),
            state_update: None,
        };
        let sub_context = super::TurnContext {
            conversation_id: ctx.conversation_id.to_string(),
            messages,
            tools,
            prior_cost: self.cumulative_cost,
            depth: 1,
        };
        let sub_services = super::TurnServices {
            mcp: self.services.mcp,
            fetch_config: self.services.fetch_config,
            filesystem_config: self.services.filesystem_config,
            task_store: self.services.task_store,
            pending_questions: self.services.pending_questions,
            process_manager: None,
            bg_sub_agent_deps: None,
            control_plane: self.services.control_plane.clone(),
            modules: Arc::clone(&self.services.modules),
        };
        // Sub-agent gets its own emitter with a fresh run_id
        let sub_emitter = TurnEmitter::new(
            ctx.emitter.sender().clone(),
            ctx.emitter.thread_id().to_string(),
            uuid::Uuid::new_v4().to_string(),
        );
        let result = super::run_agent_turn(
            &sub_inference,
            sub_context,
            &sub_services,
            &sub_emitter,
            ctx.cancel.clone(),
        )
        .await;

        let (content, is_error) = match result {
            Ok(turn_result) => {
                let text = extract_text_from_messages(&turn_result.messages);
                let summary = if text.len() > 200 {
                    format!("{}...", &text[..200])
                } else {
                    text.clone()
                };

                ctx.emitter.sub_agent_end(&config.agent_type.to_string(), serde_json::json!({
                    "summary": summary,
                    "input_tokens": turn_result.input_tokens,
                    "output_tokens": turn_result.output_tokens,
                }));

                if let Some(err) = turn_result.error {
                    (
                        format!("Sub-agent encountered an error: {}\n\nPartial output:\n{}", err, text),
                        true,
                    )
                } else {
                    (text, false)
                }
            }
            Err(e) => {
                ctx.emitter.sub_agent_end(&config.agent_type.to_string(), serde_json::json!({
                    "error": e.to_string(),
                }));
                (
                    format!("Sub-agent failed: {}", e),
                    true,
                )
            }
        };

        ToolResult { content, is_error, injected_messages: Vec::new() }
    }
}

impl SubAgentHandler<'_> {
    async fn spawn_background_sub_agent(
        &self,
        ctx: &ToolContext<'_>,
        config: &SubAgentConfig,
    ) -> ToolResult {
        let bg_deps = match &self.services.bg_sub_agent_deps {
            Some(deps) => Arc::clone(deps),
            None => {
                return ToolResult {
                    content: "Background sub-agent dispatch is not available at this depth."
                        .to_string(),
                    is_error: true,
                injected_messages: Vec::new(),
                };
            }
        };

        let pm = &bg_deps.turns.process_manager;
        let label = format!("{} sub-agent: {}", config.agent_type,
            if config.task.len() > 50 { &config.task[..50] } else { &config.task });

        let spawn_result = match pm
            .spawn(ctx.conversation_id, label, config.task.clone(), ProcessKind::SubAgent)
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::error(e),
        };

        // Build messages for the background sub-agent
        let messages = match config.context_mode {
            ContextMode::Fresh => vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: config.task.clone(),
                }],
            }],
            ContextMode::Branched => {
                let mut msgs = self.parent_messages.to_vec();
                msgs.push(Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: config.task.clone(),
                    }],
                });
                msgs
            }
        };

        let tools = filter_tools_for_sub_agent(self.parent_tools, config);
        let system_prompt = config.system_prompt.clone();
        let model = self.inference.model.to_string();
        let max_tokens = self.inference.max_tokens;
        let temperature = self.inference.temperature;
        let cumulative_cost = self.cumulative_cost;
        let process_id = spawn_result.process_id.clone();
        let cancel_token = spawn_result.cancel_token;
        let output_path = spawn_result.output_path;
        let conversation_id = ctx.conversation_id.to_string();

        tokio::spawn(async move {
            let bg_tx = bg_deps.turns.event_bridge.agent_tx();
            let bg_emitter = TurnEmitter::new(
                bg_tx,
                conversation_id.clone(),
                uuid::Uuid::new_v4().to_string(),
            );
            let mcp_guard = bg_deps.mcp.mcp.read().await;

            let bg_inference = super::InferenceConfig {
                provider: bg_deps.provider.as_ref(),
                model: &model,
                max_tokens,
                temperature,
                thinking_budget: None,
                system_prompt: Some(system_prompt),
                state_update: None,
            };
            let bg_context = super::TurnContext {
                conversation_id: conversation_id.clone(),
                messages,
                tools,
                prior_cost: cumulative_cost,
                depth: 1,
            };
            let bg_services = super::TurnServices {
                mcp: &mcp_guard,
                fetch_config: &bg_deps.fetch_config,
                filesystem_config: &bg_deps.filesystem_config,
                task_store: bg_deps.tasks.store(),
                pending_questions: &bg_deps.turns.pending_questions,
                process_manager: Some(bg_deps.turns.process_manager.clone()),
                bg_sub_agent_deps: None,
                control_plane: None,
                modules: Arc::clone(&bg_deps.modules),
            };

            let result = tokio::select! {
                result = super::run_agent_turn(
                    &bg_inference, bg_context, &bg_services, &bg_emitter, cancel_token.clone(),
                ) => result,
                _ = cancel_token.cancelled() => {
                    Err(anyhow::anyhow!("Cancelled"))
                }
            };

            drop(mcp_guard);

            let (output, is_error) = match result {
                Ok(turn_result) => {
                    let text = extract_text_from_messages(&turn_result.messages);
                    if let Some(err) = turn_result.error {
                        (format!("Error: {}\n\n{}", err, text), true)
                    } else {
                        (text, false)
                    }
                }
                Err(e) => (format!("Sub-agent failed: {}", e), true),
            };

            let _ = tokio::fs::write(&output_path, &output).await;
            bg_deps
                .turns
                .process_manager
                .complete(&process_id, None, is_error)
                .await;
        });

        ToolResult {
            content: serde_json::json!({
                "process_id": spawn_result.process_id,
                "status": "running",
                "message": "Sub-agent started in background. You will be notified when it completes. Use process_output to read output."
            })
            .to_string(),
            is_error: false,
            injected_messages: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_minimal_args() {
        let args = serde_json::json!({ "task": "find auth files" });
        let config = SubAgentConfig::from_args(&args).unwrap();
        assert_eq!(config.agent_type, SubAgentType::Explore);
        assert_eq!(config.context_mode, ContextMode::Fresh);
        assert_eq!(config.task, "find auth files");
        assert!(!config.include_task_tools);
    }

    #[test]
    fn config_from_full_args() {
        let args = serde_json::json!({
            "task": "implement the feature",
            "agent_type": "execute",
            "context": "branched",
        });
        let config = SubAgentConfig::from_args(&args).unwrap();
        assert_eq!(config.agent_type, SubAgentType::Execute);
        assert_eq!(config.context_mode, ContextMode::Branched);
        assert!(config.include_task_tools);
    }

    #[test]
    fn config_custom_requires_system_prompt() {
        let args = serde_json::json!({
            "task": "do something",
            "agent_type": "custom",
        });
        let err = SubAgentConfig::from_args(&args).unwrap_err();
        assert!(err.contains("system_prompt is required"));
    }

    #[test]
    fn config_custom_with_system_prompt() {
        let args = serde_json::json!({
            "task": "do something",
            "agent_type": "custom",
            "system_prompt": "You are a security auditor.",
        });
        let config = SubAgentConfig::from_args(&args).unwrap();
        assert_eq!(config.system_prompt, "You are a security auditor.");
    }

    #[test]
    fn missing_task_is_error() {
        let args = serde_json::json!({ "agent_type": "explore" });
        assert!(SubAgentConfig::from_args(&args).is_err());
    }

    #[test]
    fn filter_excludes_sub_agent_tool() {
        let tools = vec![
            Tool {
                name: "ask_user".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type": "object"}),
            },
            Tool {
                name: "sub_agent".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type": "object"}),
            },
            Tool {
                name: "mcp_read_file".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type": "object"}),
            },
        ];
        let config = SubAgentConfig::from_args(&serde_json::json!({ "task": "x" })).unwrap();
        let filtered = filter_tools_for_sub_agent(&tools, &config);
        let names: Vec<&str> = filtered.iter().map(|t| t.name.as_str()).collect();
        assert!(!names.contains(&"sub_agent"));
        assert!(names.contains(&"ask_user"));
        assert!(names.contains(&"mcp_read_file"));
    }

    #[test]
    fn filter_excludes_task_tools_for_explore() {
        let tools = vec![
            Tool {
                name: "task_create_plan".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type": "object"}),
            },
            Tool {
                name: "task_list".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type": "object"}),
            },
            Tool {
                name: "mcp_read_file".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type": "object"}),
            },
        ];
        let config = SubAgentConfig::from_args(&serde_json::json!({ "task": "x" })).unwrap();
        let filtered = filter_tools_for_sub_agent(&tools, &config);
        let names: Vec<&str> = filtered.iter().map(|t| t.name.as_str()).collect();
        assert!(!names.contains(&"task_create_plan"));
        assert!(!names.contains(&"task_list"));
        assert!(names.contains(&"mcp_read_file"));
    }

    #[test]
    fn filter_includes_task_tools_for_plan() {
        let tools = vec![
            Tool {
                name: "task_create_plan".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type": "object"}),
            },
            Tool {
                name: "mcp_read_file".into(),
                description: String::new(),
                input_schema: serde_json::json!({"type": "object"}),
            },
        ];
        let args = serde_json::json!({ "task": "x", "agent_type": "plan" });
        let config = SubAgentConfig::from_args(&args).unwrap();
        let filtered = filter_tools_for_sub_agent(&tools, &config);
        let names: Vec<&str> = filtered.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"task_create_plan"));
    }

    #[test]
    fn extract_text_from_empty_messages() {
        let text = extract_text_from_messages(&[]);
        assert_eq!(text, "(Sub-agent produced no text output)");
    }

    #[test]
    fn extract_text_skips_user_messages() {
        let messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "user text".into(),
            }],
        }];
        let text = extract_text_from_messages(&messages);
        assert_eq!(text, "(Sub-agent produced no text output)");
    }

    #[test]
    fn extract_text_joins_assistant_blocks() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "First finding.".into(),
                }],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "result".into(),
                    is_error: None,
                }],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "Second finding.".into(),
                }],
            },
        ];
        let text = extract_text_from_messages(&messages);
        assert_eq!(text, "First finding.\n\nSecond finding.");
    }
}
