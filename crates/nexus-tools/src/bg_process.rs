use async_trait::async_trait;
use nexus_provider::types::Tool;

const PROCESS_OUTPUT: &str = "process_output";
const PROCESS_STATUS: &str = "process_status";
const PROCESS_STOP: &str = "process_stop";

pub fn is_bg_process_tool(name: &str) -> bool {
    matches!(name, PROCESS_OUTPUT | PROCESS_STATUS | PROCESS_STOP)
}

/// Backend trait abstracting process management operations.
/// The daemon implements this on its ProcessManager.
#[async_trait]
pub trait ProcessBackend: Send + Sync {
    async fn read_output(
        &self,
        process_id: &str,
        tail: Option<usize>,
        head: Option<usize>,
    ) -> Result<String, String>;
    async fn list_json(&self, conversation_id: &str) -> String;
    async fn cancel(&self, process_id: &str) -> Result<(), String>;
}

/// Execute a bg_process tool call. Returns (content, is_error).
pub async fn execute(
    tool_name: &str,
    args: &serde_json::Value,
    conversation_id: &str,
    backend: &dyn ProcessBackend,
) -> (String, bool) {
    match tool_name {
        "process_output" => {
            let Some(process_id) = args.get("process_id").and_then(|v| v.as_str()) else {
                return ("Missing required field: 'process_id'".to_string(), true);
            };
            let tail = args.get("tail").and_then(|v| v.as_u64()).map(|n| n as usize);
            let head = args.get("head").and_then(|v| v.as_u64()).map(|n| n as usize);
            match backend.read_output(process_id, tail, head).await {
                Ok(output) => (output, false),
                Err(e) => (e, true),
            }
        }
        "process_status" => {
            let content = backend.list_json(conversation_id).await;
            (content, false)
        }
        "process_stop" => {
            let Some(process_id) = args.get("process_id").and_then(|v| v.as_str()) else {
                return ("Missing required field: 'process_id'".to_string(), true);
            };
            match backend.cancel(process_id).await {
                Ok(()) => (format!("Process {} stopped.", process_id), false),
                Err(e) => (e, true),
            }
        }
        _ => (format!("Unknown bg_process tool: {}", tool_name), true),
    }
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
