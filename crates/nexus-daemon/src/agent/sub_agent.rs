use async_trait::async_trait;
use serde::Deserialize;

use crate::anthropic::types::{ContentBlock, Message, Role, Tool};
use crate::ask_user::PendingQuestionStore;
use crate::config::{FetchConfig, FilesystemConfig};
use crate::mcp::McpManager;
use crate::provider::InferenceProvider;
use crate::tasks::store::TaskStateStore;

use super::events::AgUiEvent;
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
                }
            },
            "required": ["task"]
        }),
    }
}

// ── Agent type presets ──

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentType {
    Explore,
    Plan,
    Execute,
    Custom,
}

impl Default for SubAgentType {
    fn default() -> Self {
        Self::Explore
    }
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

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    Fresh,
    Branched,
}

impl Default for ContextMode {
    fn default() -> Self {
        Self::Fresh
    }
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

// ── Handler ──

pub struct SubAgentHandler<'a> {
    pub provider: &'a dyn InferenceProvider,
    pub model: &'a str,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub mcp: &'a McpManager,
    pub fetch_config: &'a FetchConfig,
    pub filesystem_config: &'a FilesystemConfig,
    pub task_store: &'a tokio::sync::RwLock<TaskStateStore>,
    pub pending_questions: &'a tokio::sync::RwLock<PendingQuestionStore>,
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
                };
            }
        };

        // Emit sub_agent_start event
        let _ = ctx.tx.send(AgUiEvent::Custom {
            thread_id: ctx.conversation_id.to_string(),
            name: "sub_agent_start".to_string(),
            value: serde_json::json!({
                "agent_type": config.agent_type.to_string(),
                "task": config.task,
                "context": if config.context_mode == ContextMode::Branched { "branched" } else { "fresh" },
            }),
        });

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
        let result = super::run_agent_turn(
            self.provider,
            ctx.conversation_id,
            messages,
            tools,
            Some(config.system_prompt),
            None, // sub-agents don't use state injection
            self.model,
            self.max_tokens,
            self.temperature,
            None, // sub-agents don't use extended thinking
            self.mcp,
            self.fetch_config,
            self.filesystem_config,
            self.task_store,
            self.pending_questions,
            ctx.tx,
            ctx.cancel.clone(),
            1, // depth = 1: sub-agent won't get sub_agent tool
            self.cumulative_cost, // pass parent's running total for correct usage_update display
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

                // Emit sub_agent_end event
                let _ = ctx.tx.send(AgUiEvent::Custom {
                    thread_id: ctx.conversation_id.to_string(),
                    name: "sub_agent_end".to_string(),
                    value: serde_json::json!({
                        "agent_type": config.agent_type.to_string(),
                        "summary": summary,
                        "input_tokens": turn_result.input_tokens,
                        "output_tokens": turn_result.output_tokens,
                    }),
                });

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
                let _ = ctx.tx.send(AgUiEvent::Custom {
                    thread_id: ctx.conversation_id.to_string(),
                    name: "sub_agent_end".to_string(),
                    value: serde_json::json!({
                        "agent_type": config.agent_type.to_string(),
                        "error": e.to_string(),
                    }),
                });
                (
                    format!("Sub-agent failed: {}", e),
                    true,
                )
            }
        };

        ToolResult { content, is_error }
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
