use serde_json::Value;

/// Events emitted during agent execution, for UI streaming.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    TurnStart { turn: usize },
    Thinking { content: String },
    Text { content: String },
    ToolCall { name: String, input: Value },
    ToolResult { name: String, output: String, is_error: bool },
    Compacted { pre_tokens: u32, post_tokens: u32 },
    Finished { turns: usize },
    Error { message: String },
}
