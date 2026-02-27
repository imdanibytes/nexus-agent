use anyhow::Result;

use crate::anthropic::types::{ContentBlock, Message, MessagesRequest, Role};
use crate::anthropic::AnthropicClient;
use crate::conversation::types::{ChatMessage, MessagePart, MessageRole};

const SUMMARIZE_MODEL: &str = "claude-sonnet-4-6";
const SUMMARIZE_MAX_TOKENS: u32 = 2048;

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

/// Find a safe split point that doesn't separate tool calls from their results.
///
/// Starts at `messages.len() - keep_recent` and advances forward if the
/// boundary would orphan a ToolResult from its matching ToolCall.
fn safe_split_point(messages: &[&ChatMessage], keep_recent: usize) -> usize {
    let mut split_at = messages.len() - keep_recent;

    // If the last consumed message is an assistant with tool calls,
    // the next message contains the matching tool results — include it
    // in the consumed set to avoid orphaned ToolResults in the kept set.
    if split_at > 0 && split_at < messages.len() {
        let last_consumed = messages[split_at - 1];
        if last_consumed.role == MessageRole::Assistant
            && last_consumed
                .parts
                .iter()
                .any(|p| matches!(p, MessagePart::ToolCall { .. }))
        {
            split_at += 1;
        }
    }

    split_at
}

/// Summarize old messages into a compact structured reference.
///
/// Keeps the last `keep_recent` messages intact. Everything before is fed
/// to Sonnet for summarization. Returns the summary text and the list of
/// consumed message IDs. The caller is responsible for creating spans
/// and updating `active_path`.
pub async fn summarize_messages(
    client: &AnthropicClient,
    messages: &[&ChatMessage],
    keep_recent: usize,
) -> Result<(String, Vec<String>)> {
    if messages.len() <= keep_recent {
        anyhow::bail!("Not enough messages to summarize");
    }

    let split_at = safe_split_point(messages, keep_recent);

    // After boundary adjustment we may have consumed too many messages
    if split_at == 0 {
        anyhow::bail!("No messages to summarize after safe boundary adjustment");
    }

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
        thinking: None,
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

    let consumed_ids: Vec<String> = to_summarize.iter().map(|m| m.id.clone()).collect();

    tracing::info!(
        consumed = consumed_ids.len(),
        kept = keep_recent,
        summary_chars = summary_text.len(),
        "Conversation compacted"
    );

    Ok((summary_text, consumed_ids))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chat_msg(id: &str, role: MessageRole, parts: Vec<MessagePart>) -> ChatMessage {
        ChatMessage {
            id: id.to_string(),
            role,
            parts,
            timestamp: chrono::Utc::now(),
            parent_id: None,
            source: None,
            metadata: None,
        }
    }

    fn text_user(id: &str, text: &str) -> ChatMessage {
        make_chat_msg(
            id,
            MessageRole::User,
            vec![MessagePart::Text {
                text: text.to_string(),
            }],
        )
    }

    fn text_assistant(id: &str, text: &str) -> ChatMessage {
        make_chat_msg(
            id,
            MessageRole::Assistant,
            vec![MessagePart::Text {
                text: text.to_string(),
            }],
        )
    }

    fn tool_call_assistant(id: &str, tool_call_id: &str) -> ChatMessage {
        make_chat_msg(
            id,
            MessageRole::Assistant,
            vec![MessagePart::ToolCall {
                tool_call_id: tool_call_id.to_string(),
                tool_name: "read_file".to_string(),
                args: serde_json::json!({}),
                result: None,
                is_error: false,
            }],
        )
    }

    fn tool_result_user(id: &str, tool_call_id: &str) -> ChatMessage {
        make_chat_msg(
            id,
            MessageRole::User,
            vec![MessagePart::ToolResult {
                tool_call_id: tool_call_id.to_string(),
                result: "ok".to_string(),
                is_error: false,
            }],
        )
    }

    #[test]
    fn safe_split_no_tools() {
        let msgs = vec![
            text_user("1", "hi"),
            text_assistant("2", "hello"),
            text_user("3", "how"),
            text_assistant("4", "fine"),
            text_user("5", "bye"),
        ];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        assert_eq!(safe_split_point(&refs, 2), 3);
    }

    #[test]
    fn safe_split_boundary_between_tool_call_and_result() {
        let msgs = vec![
            text_user("1", "do something"),
            tool_call_assistant("2", "tool_0"),
            tool_result_user("3", "tool_0"),
            text_user("4", "thanks"),
            text_assistant("5", "done"),
        ];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        assert_eq!(safe_split_point(&refs, 3), 3);
    }

    #[test]
    fn safe_split_boundary_after_tool_result() {
        let msgs = vec![
            text_user("1", "do something"),
            tool_call_assistant("2", "tool_0"),
            tool_result_user("3", "tool_0"),
            text_user("4", "thanks"),
            text_assistant("5", "done"),
        ];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        assert_eq!(safe_split_point(&refs, 2), 3);
    }

    #[test]
    fn safe_split_boundary_on_text_assistant() {
        let msgs = vec![
            text_user("1", "hi"),
            text_assistant("2", "hello"),
            text_user("3", "how"),
            text_assistant("4", "fine"),
        ];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        assert_eq!(safe_split_point(&refs, 2), 2);
    }
}
