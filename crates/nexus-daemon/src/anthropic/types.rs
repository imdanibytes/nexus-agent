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
    pub message: String,
}
