use serde_json::Value;

/// Fully-formed request — the provider just sends it.
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: Option<String>,
    pub tools: Vec<Value>,
    pub messages: Vec<Value>,
    /// Extended thinking configuration. None = disabled.
    pub thinking: Option<ThinkingConfig>,
}

/// Configuration for Claude's extended thinking.
#[derive(Debug, Clone)]
pub struct ThinkingConfig {
    /// Token budget for thinking. The model uses up to this many tokens
    /// for its internal reasoning before producing a response.
    pub budget_tokens: u32,
}

/// What came back from the LLM.
#[derive(Debug, Clone)]
pub struct InferenceResponse {
    pub stop_reason: StopReason,
    pub content: Vec<ContentBlock>,
    pub usage: Usage,
}

/// Why the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}

/// A content block in the model's response.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text(String),
    Thinking(String),
    ToolUse { id: String, name: String, input: Value },
}

/// Token usage for a single inference call.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl Usage {
    pub fn accumulate(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}
