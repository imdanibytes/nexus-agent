//! Context compaction: tool result pruning and LLM summarization.
//!
//! Two layers keep conversations within the context window:
//!
//! 1. **Tool result pruning** — mechanical, no LLM call. Replaces old tool
//!    result content with compact stubs. Non-destructive (operates on the
//!    API message array, not stored ChatMessages).
//!
//! 2. **LLM summarization** — Sonnet call to summarize old messages into a
//!    compact reference. Permanent: replaces messages in the stored conversation.

mod pruning;
mod summarize;

pub use pruning::prune_tool_results;
pub use summarize::summarize_messages;

use crate::anthropic::types::{ContentBlock, Message, Tool};

// ── Constants ──

/// Fraction of context window above which pruning activates.
pub const PRUNE_THRESHOLD_PCT: f64 = 0.5;

/// Fraction of effective window above which summarization activates.
pub const SUMMARIZE_THRESHOLD_PCT: f64 = 0.8;

// ── Token Estimation ──

/// Estimate token count from API messages, system prompt, and tools.
///
/// Uses a chars/3 heuristic. Slightly overestimates, which is desirable —
/// better to trigger compaction a bit early than hit the context limit.
pub fn estimate_tokens(
    messages: &[Message],
    system_prompt: Option<&str>,
    tools: &[Tool],
) -> u32 {
    let mut chars: usize = 0;

    if let Some(sp) = system_prompt {
        chars += sp.len();
    }

    for msg in messages {
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => chars += text.len(),
                ContentBlock::ToolUse { name, input, .. } => {
                    chars += name.len();
                    chars += input.to_string().len();
                }
                ContentBlock::ToolResult { content, .. } => chars += content.len(),
                ContentBlock::Thinking { thinking } => chars += thinking.len(),
            }
        }
    }

    for tool in tools {
        chars += tool.name.len() + tool.description.len();
        chars += tool.input_schema.to_string().len();
    }

    (chars / 3) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::types::Role;

    #[test]
    fn estimate_tokens_basic() {
        let messages = vec![
            Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "Hello world".to_string(),
                }],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "Hi there, how can I help?".to_string(),
                }],
            },
        ];
        let tools = vec![Tool {
            name: "read_file".to_string(),
            description: "Read a file from disk".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        }];

        let estimate = estimate_tokens(&messages, Some("System prompt here"), &tools);
        assert!(estimate > 0);
        assert!(estimate < 200);
    }

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(&[], None, &[]), 0);
    }
}
