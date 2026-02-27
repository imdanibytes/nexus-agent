use async_trait::async_trait;

use crate::agent::tool_dispatch::{ToolContext, ToolHandler, ToolResult};
use crate::anthropic::types::Tool;
use super::manager::ProcessManager;

const PROCESS_OUTPUT: &str = "process_output";
const PROCESS_STATUS: &str = "process_status";
const PROCESS_STOP: &str = "process_stop";

pub fn is_bg_process_tool(name: &str) -> bool {
    matches!(name, PROCESS_OUTPUT | PROCESS_STATUS | PROCESS_STOP)
}

pub fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool {
            name: PROCESS_OUTPUT.to_string(),
            description: "Read output from a background process. Returns the stdout/stderr \
                captured to disk. Use tail or head to read specific sections of large output."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "process_id": {
                        "type": "string",
                        "description": "The ID of the background process to read output from."
                    },
                    "tail": {
                        "type": "integer",
                        "description": "Read last N lines of output."
                    },
                    "head": {
                        "type": "integer",
                        "description": "Read first N lines of output."
                    }
                },
                "required": ["process_id"]
            }),
        },
        Tool {
            name: PROCESS_STATUS.to_string(),
            description: "List all background processes for the current conversation. \
                Shows status, labels, and output sizes."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: PROCESS_STOP.to_string(),
            description: "Stop a running background process by its ID."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "process_id": {
                        "type": "string",
                        "description": "The ID of the background process to stop."
                    }
                },
                "required": ["process_id"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_bg_process_tool_recognizes_all_tools() {
        assert!(is_bg_process_tool("process_output"));
        assert!(is_bg_process_tool("process_status"));
        assert!(is_bg_process_tool("process_stop"));
    }

    #[test]
    fn is_bg_process_tool_rejects_others() {
        assert!(!is_bg_process_tool("bash"));
        assert!(!is_bg_process_tool("read_file"));
        assert!(!is_bg_process_tool("process_"));
        assert!(!is_bg_process_tool(""));
    }

    #[test]
    fn tool_definitions_count() {
        let defs = tool_definitions();
        assert_eq!(defs.len(), 3);

        let names: Vec<&str> = defs.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"process_output"));
        assert!(names.contains(&"process_status"));
        assert!(names.contains(&"process_stop"));
    }

    #[test]
    fn process_output_schema_requires_process_id() {
        let defs = tool_definitions();
        let output_tool = defs.iter().find(|t| t.name == "process_output").unwrap();
        let required = output_tool.input_schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("process_id")));
    }
}

pub struct BgProcessToolHandler<'a> {
    pub process_manager: &'a ProcessManager,
}

#[async_trait]
impl ToolHandler for BgProcessToolHandler<'_> {
    fn can_handle(&self, tool_name: &str) -> bool {
        is_bg_process_tool(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        let args: serde_json::Value = serde_json::from_str(ctx.args_json)
            .unwrap_or_else(|_| serde_json::json!({}));

        match ctx.tool_name {
            PROCESS_OUTPUT => {
                let process_id = match args.get("process_id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => {
                        return ToolResult {
                            content: "Missing required field: 'process_id'".to_string(),
                            is_error: true,
                        lsp_diagnostics: None,
                        };
                    }
                };
                let tail = args.get("tail").and_then(|v| v.as_u64()).map(|n| n as usize);
                let head = args.get("head").and_then(|v| v.as_u64()).map(|n| n as usize);

                match self.process_manager.read_output(process_id, tail, head).await {
                    Ok(output) => ToolResult {
                        content: output,
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
            PROCESS_STATUS => {
                let processes = self.process_manager.list(ctx.conversation_id).await;
                let content = serde_json::to_string_pretty(&processes).unwrap_or_default();
                ToolResult {
                    content,
                    is_error: false,
                lsp_diagnostics: None,
                }
            }
            PROCESS_STOP => {
                let process_id = match args.get("process_id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => {
                        return ToolResult {
                            content: "Missing required field: 'process_id'".to_string(),
                            is_error: true,
                        lsp_diagnostics: None,
                        };
                    }
                };
                match self.process_manager.cancel(process_id).await {
                    Ok(()) => ToolResult {
                        content: format!("Process {} stopped.", process_id),
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
            _ => ToolResult {
                content: format!("Unknown bg_process tool: {}", ctx.tool_name),
                is_error: true,
                lsp_diagnostics: None,
            },
        }
    }
}
