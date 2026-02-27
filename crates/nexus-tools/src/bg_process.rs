use nexus_provider::types::Tool;

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
