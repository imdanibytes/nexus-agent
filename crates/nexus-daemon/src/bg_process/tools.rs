pub use nexus_tools::bg_process::{is_bg_process_tool, tool_definitions};

use async_trait::async_trait;

use crate::agent::tool_dispatch::{ToolContext, ToolHandler, ToolResult};
use super::manager::ProcessManager;

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
            "process_output" => {
                let process_id = match args.get("process_id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => {
                        return ToolResult {
                            content: "Missing required field: 'process_id'".to_string(),
                            is_error: true,
                        injected_messages: Vec::new(),
                        };
                    }
                };
                let tail = args.get("tail").and_then(|v| v.as_u64()).map(|n| n as usize);
                let head = args.get("head").and_then(|v| v.as_u64()).map(|n| n as usize);

                match self.process_manager.read_output(process_id, tail, head).await {
                    Ok(output) => ToolResult {
                        content: output,
                        is_error: false,
                    injected_messages: Vec::new(),
                    },
                    Err(e) => ToolResult {
                        content: e,
                        is_error: true,
                    injected_messages: Vec::new(),
                    },
                }
            }
            "process_status" => {
                let processes = self.process_manager.list(ctx.conversation_id).await;
                let content = serde_json::to_string_pretty(&processes).unwrap_or_default();
                ToolResult {
                    content,
                    is_error: false,
                injected_messages: Vec::new(),
                }
            }
            "process_stop" => {
                let process_id = match args.get("process_id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => {
                        return ToolResult {
                            content: "Missing required field: 'process_id'".to_string(),
                            is_error: true,
                        injected_messages: Vec::new(),
                        };
                    }
                };
                match self.process_manager.cancel(process_id).await {
                    Ok(()) => ToolResult {
                        content: format!("Process {} stopped.", process_id),
                        is_error: false,
                        injected_messages: Vec::new(),
                    },
                    Err(e) => ToolResult {
                        content: e,
                        is_error: true,
                    injected_messages: Vec::new(),
                    },
                }
            }
            _ => ToolResult {
                content: format!("Unknown bg_process tool: {}", ctx.tool_name),
                is_error: true,
                injected_messages: Vec::new(),
            },
        }
    }
}
