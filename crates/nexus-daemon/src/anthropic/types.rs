use serde::{Deserialize, Serialize};

// ── Request types ──

#[derive(Debug, Clone, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    Thinking {
        thinking: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Inject a required `description` field into every tool's input_schema.
///
/// This forces the model to articulate its reasoning before acting,
/// improving tool-use quality and providing an audit trail.
/// Applied as post-processing so individual tool definitions stay clean.
pub fn inject_tool_description_field(tools: &mut [Tool]) {
    let desc_schema = serde_json::json!({
        "type": "string",
        "description": "Brief explanation of what you're doing and why (1-2 sentences)."
    });

    for tool in tools.iter_mut() {
        if let Some(schema) = tool.input_schema.as_object_mut() {
            // Add description to properties
            if let Some(props) = schema.get_mut("properties").and_then(|p| p.as_object_mut()) {
                props.insert("description".to_string(), desc_schema.clone());
            }
            // Add to required array
            if let Some(required) = schema.get_mut("required").and_then(|r| r.as_array_mut()) {
                let val = serde_json::Value::String("description".to_string());
                if !required.contains(&val) {
                    required.push(val);
                }
            } else {
                // No required array yet — create one with description
                schema.insert(
                    "required".to_string(),
                    serde_json::json!(["description"]),
                );
            }
        }
    }
}

// ── SSE event types (streaming response) ──

#[derive(Debug, Clone)]
pub enum StreamEvent {
    MessageStart {
        message_id: String,
        model: String,
        role: Role,
        usage: Option<Usage>,
    },
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockInfo,
    },
    ContentBlockDelta {
        index: usize,
        delta: Delta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        stop_reason: Option<StopReason>,
        usage: Option<Usage>,
    },
    MessageStop,
    Ping,
    Error {
        error_type: Option<String>,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum ContentBlockInfo {
    Text,
    ToolUse { id: String, name: String },
    Thinking,
}

#[derive(Debug, Clone)]
pub enum Delta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

// ── Prompt caching ──

/// Inject prompt caching breakpoints into a serialized API request body.
///
/// Modifies the JSON in-place to add `cache_control: {"type": "ephemeral"}`
/// at up to 3 positions (out of the 4 allowed):
/// 1. System prompt — converted from string to array format, last block cached
/// 2. Last tool definition
/// 3. Last cacheable content block of the last message
///
/// The system prompt is built once per turn and cloned for each tool-use round,
/// so it's stable within a turn and benefits from caching across multi-round
/// tool use. Cross-turn cache misses on system are expected (datetime changes)
/// but the write cost is amortized by within-turn read hits.
pub fn inject_cache_control(body: &mut serde_json::Value) {
    let cc = serde_json::json!({"type": "ephemeral"});

    // System prompt: convert string → array of text blocks, cache the block.
    // The API accepts both formats; array is required for cache_control.
    if let Some(system) = body.get_mut("system") {
        if let Some(s) = system.as_str() {
            let text = s.to_string();
            *system = serde_json::json!([{
                "type": "text",
                "text": text,
                "cache_control": cc,
            }]);
        } else if let Some(arr) = system.as_array_mut() {
            if let Some(last) = arr.last_mut() {
                last["cache_control"] = cc.clone();
            }
        }
    }

    // Tools: cache the last tool definition
    if let Some(tools) = body.get_mut("tools") {
        if let Some(arr) = tools.as_array_mut() {
            if let Some(last) = arr.last_mut() {
                last["cache_control"] = cc.clone();
            }
        }
    }

    // Messages: cache the last message's last cacheable content block.
    // Thinking blocks can't be directly cached, so we skip them.
    if let Some(messages) = body.get_mut("messages") {
        if let Some(arr) = messages.as_array_mut() {
            if let Some(last_msg) = arr.last_mut() {
                if let Some(content) = last_msg.get_mut("content") {
                    if let Some(blocks) = content.as_array_mut() {
                        if let Some(block) = blocks
                            .iter_mut()
                            .rev()
                            .find(|b| {
                                b.get("type").and_then(|t| t.as_str()) != Some("thinking")
                            })
                        {
                            block["cache_control"] = cc;
                        }
                    }
                }
            }
        }
    }
}

// ── Raw SSE JSON shapes (for deserialization only) ──

#[derive(Debug, Deserialize)]
pub(crate) struct RawMessageStart {
    pub message: RawMessageInfo,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawMessageInfo {
    pub id: String,
    pub model: String,
    pub role: Role,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawContentBlockStart {
    pub index: usize,
    pub content_block: RawContentBlock,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum RawContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String },
    Thinking { thinking: String },
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawContentBlockDelta {
    pub index: usize,
    pub delta: RawDelta,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum RawDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawContentBlockStop {
    pub index: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawMessageDelta {
    pub delta: RawMessageDeltaInner,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawMessageDeltaInner {
    pub stop_reason: Option<StopReason>,
}

// ── Non-streaming response ──

#[derive(Debug, Clone, Deserialize)]
pub struct MessagesResponse {
    pub id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<StopReason>,
    pub usage: Option<Usage>,
}

// ── Raw SSE JSON shapes (continued) ──

#[derive(Debug, Deserialize)]
pub(crate) struct RawError {
    pub error: RawErrorInner,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawErrorInner {
    #[serde(rename = "type")]
    pub error_type: Option<String>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_cache_control_converts_system_string_to_array() {
        let mut body = serde_json::json!({
            "model": "claude-sonnet-4-6",
            "system": "You are helpful.",
            "messages": [],
            "tools": [],
        });
        inject_cache_control(&mut body);

        let system = body["system"].as_array().expect("system should be array");
        assert_eq!(system.len(), 1);
        assert_eq!(system[0]["type"], "text");
        assert_eq!(system[0]["text"], "You are helpful.");
        assert_eq!(system[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn inject_cache_control_caches_last_tool() {
        let mut body = serde_json::json!({
            "model": "claude-sonnet-4-6",
            "messages": [],
            "tools": [
                {"name": "tool_a", "description": "first"},
                {"name": "tool_b", "description": "second"},
            ],
        });
        inject_cache_control(&mut body);

        let tools = body["tools"].as_array().unwrap();
        // First tool should NOT have cache_control
        assert!(tools[0].get("cache_control").is_none());
        // Last tool should have cache_control
        assert_eq!(tools[1]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn inject_cache_control_caches_last_message_content_block() {
        let mut body = serde_json::json!({
            "model": "claude-sonnet-4-6",
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "hello"}]
                },
                {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "response"},
                        {"type": "tool_use", "id": "t1", "name": "fetch", "input": {}}
                    ]
                }
            ],
            "tools": [],
        });
        inject_cache_control(&mut body);

        let messages = body["messages"].as_array().unwrap();
        // First message: no cache_control on its content
        assert!(messages[0]["content"][0].get("cache_control").is_none());
        // Last message, last block (tool_use): should have cache_control
        assert_eq!(
            messages[1]["content"][1]["cache_control"]["type"],
            "ephemeral"
        );
        // Last message, first block: no cache_control
        assert!(messages[1]["content"][0].get("cache_control").is_none());
    }

    #[test]
    fn inject_cache_control_skips_thinking_blocks() {
        let mut body = serde_json::json!({
            "model": "claude-sonnet-4-6",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "before thinking"},
                        {"type": "thinking", "thinking": "internal thought"}
                    ]
                }
            ],
            "tools": [],
        });
        inject_cache_control(&mut body);

        let content = body["messages"][0]["content"].as_array().unwrap();
        // Thinking block should NOT have cache_control
        assert!(content[1].get("cache_control").is_none());
        // Text block (last non-thinking) should have cache_control
        assert_eq!(content[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn inject_cache_control_handles_empty_gracefully() {
        let mut body = serde_json::json!({
            "model": "claude-sonnet-4-6",
            "messages": [],
            "tools": [],
        });
        inject_cache_control(&mut body);
        // No panic, no crash — just a no-op for empty arrays
        assert!(body["messages"].as_array().unwrap().is_empty());
    }

    #[test]
    fn usage_deserializes_cache_tokens() {
        let json = r#"{
            "input_tokens": 50,
            "output_tokens": 100,
            "cache_creation_input_tokens": 5000,
            "cache_read_input_tokens": 10000
        }"#;
        let usage: Usage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.output_tokens, 100);
        assert_eq!(usage.cache_creation_input_tokens, 5000);
        assert_eq!(usage.cache_read_input_tokens, 10000);
    }

    #[test]
    fn usage_defaults_cache_tokens_to_zero() {
        let json = r#"{"input_tokens": 50, "output_tokens": 100}"#;
        let usage: Usage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.cache_creation_input_tokens, 0);
        assert_eq!(usage.cache_read_input_tokens, 0);
    }

    #[test]
    fn inject_tool_description_adds_required_field() {
        let mut tools = vec![
            Tool {
                name: "read_file".into(),
                description: "Read a file".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            },
        ];
        inject_tool_description_field(&mut tools);

        let schema = &tools[0].input_schema;
        // description property was added
        assert!(schema["properties"]["description"]["type"].as_str() == Some("string"));
        // description is in required array
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("path")));
        assert!(required.contains(&serde_json::json!("description")));
    }

    #[test]
    fn inject_tool_description_no_duplicate() {
        let mut tools = vec![
            Tool {
                name: "test".into(),
                description: "Test".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "description": { "type": "string", "description": "custom" }
                    },
                    "required": ["description"]
                }),
            },
        ];
        inject_tool_description_field(&mut tools);

        // Should not duplicate in required
        let required = tools[0].input_schema["required"].as_array().unwrap();
        let count = required.iter().filter(|v| v.as_str() == Some("description")).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn inject_tool_description_creates_required_array() {
        let mut tools = vec![
            Tool {
                name: "list_things".into(),
                description: "List things".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        ];
        inject_tool_description_field(&mut tools);

        let required = tools[0].input_schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "description");
    }
}
