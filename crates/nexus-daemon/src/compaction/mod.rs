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

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::anthropic::types::{ContentBlock, Message, MessagesRequest, Role};
use crate::anthropic::AnthropicClient;
use crate::conversation::types::{ChatMessage, MessagePart, MessageRole};

// ── Constants ──

const KEEP_RECENT_TOOL_RESULTS: usize = 3;
const SUMMARIZE_KEEP_RECENT: usize = 10;
const SUMMARIZE_MODEL: &str = "claude-sonnet-4-6";
const SUMMARIZE_MAX_TOKENS: u32 = 2048;

/// Fraction of context window above which pruning activates.
pub const PRUNE_THRESHOLD_PCT: f64 = 0.5;

/// Fraction of effective window above which summarization activates.
pub const SUMMARIZE_THRESHOLD_PCT: f64 = 0.8;

// ── Layer 1: Tool Result Pruning ──

/// Prune old tool results from API messages to reclaim context space.
///
/// Keeps the last `keep_recent` tool results intact. Earlier results are
/// replaced with compact stubs showing tool name and content size.
/// Also stubs out the matching `ToolUse.input` args for pruned calls
/// (write_file/edit_file args can be huge).
///
/// Operates in-place on the API message array — stored ChatMessages are
/// untouched.
pub fn prune_tool_results(messages: &mut Vec<Message>, keep_recent: usize) {
    // First pass: collect (message_idx, block_idx) of every ToolResult, in order.
    let mut tool_result_positions: Vec<(usize, usize)> = Vec::new();

    for (msg_idx, msg) in messages.iter().enumerate() {
        for (block_idx, block) in msg.content.iter().enumerate() {
            if matches!(block, ContentBlock::ToolResult { .. }) {
                tool_result_positions.push((msg_idx, block_idx));
            }
        }
    }

    let total = tool_result_positions.len();
    if total <= keep_recent {
        return; // Nothing to prune
    }

    let prune_count = total - keep_recent;
    let to_prune = &tool_result_positions[..prune_count];

    // Build a map of tool_use_id → tool_name from assistant messages
    let mut tool_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for msg in messages.iter() {
        if msg.role != Role::Assistant {
            continue;
        }
        for block in &msg.content {
            if let ContentBlock::ToolUse { id, name, .. } = block {
                tool_names.insert(id.clone(), name.clone());
            }
        }
    }

    // Collect tool_use_ids that we're pruning (for stubbing their args too)
    let mut pruned_tool_use_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Second pass: replace pruned tool results with stubs
    for &(msg_idx, block_idx) in to_prune {
        let block = &messages[msg_idx].content[block_idx];
        if let ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } = block
        {
            let tool_name = tool_names
                .get(tool_use_id)
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            let char_count = content.len();
            let stub = format!("[{}: {} chars]", tool_name, char_count);
            pruned_tool_use_ids.insert(tool_use_id.clone());

            messages[msg_idx].content[block_idx] = ContentBlock::ToolResult {
                tool_use_id: tool_use_id.clone(),
                content: stub,
                is_error: *is_error,
            };
        }
    }

    // Third pass: stub out ToolUse.input for pruned tool calls
    for msg in messages.iter_mut() {
        if msg.role != Role::Assistant {
            continue;
        }
        for block in msg.content.iter_mut() {
            if let ContentBlock::ToolUse { id, input, .. } = block {
                if pruned_tool_use_ids.contains(id) {
                    *input = serde_json::json!({});
                }
            }
        }
    }

    tracing::info!(
        pruned = prune_count,
        kept = keep_recent,
        total,
        "Tool result pruning"
    );
}

// ── Layer 2: LLM Summarization ──

const SUMMARIZE_PROMPT: &str = "\
Summarize this conversation into a compact reference that preserves all \
context needed to continue the work. Include:

1. **Original request**: What the user asked for
2. **Key decisions**: Technical choices made and why
3. **Files modified**: Paths of files created, modified, or read (paths only)
4. **Current state**: What has been accomplished so far
5. **Unresolved items**: Open questions, next steps, or blockers

Be extremely concise — this summary replaces the original messages. \
Use bullet points, not prose. Omit pleasantries and filler.";

/// Summarize old messages into a compact structured reference.
///
/// Keeps the last `keep_recent` messages intact. Everything before is fed
/// to Sonnet for summarization. Returns a summary ChatMessage (role: User,
/// metadata: `is_compaction_summary: true`) and the list of consumed
/// message IDs to remove from `active_path`.
///
/// The summary ChatMessage should be prepended to `active_path` and added
/// to `conv.messages`. Old messages stay in `conv.messages` for branch
/// history but are removed from `active_path`.
pub async fn summarize_messages(
    client: &AnthropicClient,
    messages: &[&ChatMessage],
    keep_recent: usize,
) -> Result<(ChatMessage, Vec<String>)> {
    if messages.len() <= keep_recent {
        anyhow::bail!("Not enough messages to summarize");
    }

    let split_at = messages.len() - keep_recent;
    let to_summarize = &messages[..split_at];

    // Build a text representation of old messages for the summarizer
    let mut conversation_text = String::new();
    for msg in to_summarize {
        let role = match msg.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
        };
        for part in &msg.parts {
            match part {
                MessagePart::Text { text } => {
                    // Truncate very long text parts
                    let truncated: String = text.chars().take(2000).collect();
                    let suffix = if text.chars().count() > 2000 {
                        "…"
                    } else {
                        ""
                    };
                    conversation_text.push_str(&format!("{}: {}{}\n", role, truncated, suffix));
                }
                MessagePart::ToolCall {
                    tool_name, args, ..
                } => {
                    // Show tool name and key args (truncated)
                    let args_str = serde_json::to_string(args).unwrap_or_default();
                    let truncated: String = args_str.chars().take(200).collect();
                    conversation_text
                        .push_str(&format!("{}: [called {}({})]\n", role, tool_name, truncated));
                }
                MessagePart::ToolResult {
                    result, is_error, ..
                } => {
                    let prefix = if *is_error { "ERROR" } else { "result" };
                    let truncated: String = result.chars().take(500).collect();
                    let suffix = if result.chars().count() > 500 {
                        "…"
                    } else {
                        ""
                    };
                    conversation_text
                        .push_str(&format!("{}: [tool {}: {}{}]\n", role, prefix, truncated, suffix));
                }
                MessagePart::Thinking { .. } => {}
            }
        }
    }

    let request = MessagesRequest {
        model: SUMMARIZE_MODEL.to_string(),
        max_tokens: SUMMARIZE_MAX_TOKENS,
        system: Some(SUMMARIZE_PROMPT.to_string()),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: conversation_text,
            }],
        }],
        tools: Vec::new(),
        stream: false,
        temperature: Some(0.0),
    };

    let response = client.create_message(request).await?;

    let summary_text = response
        .content
        .iter()
        .find_map(|block| {
            if let ContentBlock::Text { text } = block {
                Some(text.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "[Compaction summary unavailable]".to_string());

    // Build summary ChatMessage
    let consumed_ids: Vec<String> = to_summarize.iter().map(|m| m.id.clone()).collect();

    let summary_msg = ChatMessage {
        id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![MessagePart::Text {
            text: format!(
                "[Conversation compacted — {} messages summarized]\n\n{}",
                consumed_ids.len(),
                summary_text,
            ),
        }],
        timestamp: Utc::now(),
        parent_id: None,
        metadata: Some(serde_json::json!({ "is_compaction_summary": true })),
    };

    tracing::info!(
        consumed = consumed_ids.len(),
        kept = keep_recent,
        summary_chars = summary_text.len(),
        "Conversation compacted"
    );

    Ok((summary_msg, consumed_ids))
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool_pair(id: &str, name: &str, result: &str) -> (Message, Message) {
        let assistant = Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({"path": "/some/long/path/to/file.rs"}),
            }],
        };
        let user = Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: result.to_string(),
                is_error: None,
            }],
        };
        (assistant, user)
    }

    #[test]
    fn prune_keeps_recent_results() {
        let mut messages = Vec::new();
        // Initial user message
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
        });
        // 5 tool call pairs
        for i in 0..5 {
            let (a, u) = make_tool_pair(
                &format!("tool_{}", i),
                "read_file",
                &"x".repeat(10000),
            );
            messages.push(a);
            messages.push(u);
        }

        prune_tool_results(&mut messages, 3);

        // Count full results vs stubs
        let mut full = 0;
        let mut stubs = 0;
        for msg in &messages {
            for block in &msg.content {
                if let ContentBlock::ToolResult { content, .. } = block {
                    if content.starts_with('[') {
                        stubs += 1;
                    } else {
                        full += 1;
                    }
                }
            }
        }

        assert_eq!(full, 3, "should keep 3 recent results");
        assert_eq!(stubs, 2, "should prune 2 old results");
    }

    #[test]
    fn prune_stubs_include_tool_name_and_size() {
        let mut messages = Vec::new();
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
        });
        for i in 0..4 {
            let (a, u) = make_tool_pair(
                &format!("tool_{}", i),
                "read_file",
                &"y".repeat(5000),
            );
            messages.push(a);
            messages.push(u);
        }

        prune_tool_results(&mut messages, 3);

        // First tool result should be pruned
        let first_result = messages
            .iter()
            .flat_map(|m| m.content.iter())
            .find_map(|b| {
                if let ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } = b
                {
                    if tool_use_id == "tool_0" {
                        Some(content.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap();

        assert_eq!(first_result, "[read_file: 5000 chars]");
    }

    #[test]
    fn prune_stubs_tool_use_args() {
        let mut messages = Vec::new();
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
        });
        for i in 0..4 {
            let (a, u) = make_tool_pair(
                &format!("tool_{}", i),
                "write_file",
                &"z".repeat(1000),
            );
            messages.push(a);
            messages.push(u);
        }

        prune_tool_results(&mut messages, 3);

        // First tool_use input should be stubbed
        let first_input = messages
            .iter()
            .flat_map(|m| m.content.iter())
            .find_map(|b| {
                if let ContentBlock::ToolUse { id, input, .. } = b {
                    if id == "tool_0" {
                        Some(input.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap();

        assert_eq!(first_input, serde_json::json!({}));
    }

    #[test]
    fn prune_noop_when_under_threshold() {
        let mut messages = Vec::new();
        messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
            }],
        });
        let (a, u) = make_tool_pair("tool_0", "read_file", "some content");
        messages.push(a);
        messages.push(u);

        let original_content = messages[2].content[0].clone();
        prune_tool_results(&mut messages, 3);

        // Should be unchanged
        assert_eq!(messages[2].content[0], original_content);
    }
}
