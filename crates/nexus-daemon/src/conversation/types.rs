use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::anthropic::types::{ContentBlock, Message, Role};
use crate::system_prompt::{fence_tool_result, fence_user_message};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    pub context_window: u32,
    /// Cumulative cost in USD across the entire conversation lifetime.
    /// This value only ever increases — it is never reset by compaction
    /// or new turns. Persisted to disk.
    #[serde(default)]
    pub total_cost: f64,
}

/// A sealed segment of conversation history.
///
/// When compaction fires, the current span is sealed (summary generated)
/// and a new open span is created. Sealed spans are read-only — branching
/// only works within the current open span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Monotonically increasing: 0, 1, 2, ...
    pub index: u32,
    /// Message IDs belonging to this span (active_path order at seal time).
    /// Empty for the current open span (active_path is source of truth).
    pub message_ids: Vec<String>,
    /// LLM-generated summary. None for the current open span.
    pub summary: Option<String>,
    /// When sealed. None = current open span.
    pub sealed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
    pub active_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ConversationUsage>,
    /// The agent that was last used in this conversation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Conversation spans — sealed segments behind compaction boundaries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
}

impl Conversation {
    /// Returns messages on the active path, in order.
    pub fn active_messages(&self) -> Vec<&ChatMessage> {
        let by_id: HashMap<&str, &ChatMessage> =
            self.messages.iter().map(|m| (m.id.as_str(), m)).collect();
        self.active_path
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .collect()
    }

    /// Walk parent chain from `message_id` to root, then walk down to the
    /// deepest descendant (picking the last child at each level).
    /// Returns the full path in root→leaf order.
    pub fn path_to(&self, message_id: &str) -> Vec<String> {
        let by_id: HashMap<&str, &ChatMessage> =
            self.messages.iter().map(|m| (m.id.as_str(), m)).collect();

        // Walk UP: target → root
        let mut path = Vec::new();
        let mut current = message_id;
        loop {
            match by_id.get(current) {
                Some(msg) => {
                    path.push(msg.id.clone());
                    match &msg.parent_id {
                        Some(pid) => current = pid,
                        None => break,
                    }
                }
                None => break,
            }
        }
        path.reverse();

        // Walk DOWN: target → deepest descendant (pick last child at each level)
        let mut children_map: HashMap<&str, Vec<&str>> = HashMap::new();
        for msg in &self.messages {
            let key = msg.parent_id.as_deref().unwrap_or("");
            children_map.entry(key).or_default().push(msg.id.as_str());
        }

        let mut tip = message_id;
        loop {
            match children_map.get(tip) {
                Some(children) if !children.is_empty() => {
                    let last = children[children.len() - 1];
                    path.push(last.to_string());
                    tip = last;
                }
                _ => break,
            }
        }

        path
    }

    /// Walk parent chain from `message_id` to root (does NOT descend to children).
    /// Returns path in root→target order.
    pub fn path_to_only(&self, message_id: &str) -> Vec<String> {
        let by_id: HashMap<&str, &ChatMessage> =
            self.messages.iter().map(|m| (m.id.as_str(), m)).collect();
        let mut path = Vec::new();
        let mut current = message_id;
        loop {
            match by_id.get(current) {
                Some(msg) => {
                    path.push(msg.id.clone());
                    match &msg.parent_id {
                        Some(pid) => current = pid,
                        None => break,
                    }
                }
                None => break,
            }
        }
        path.reverse();
        path
    }

    /// Returns all child message IDs for a given parent_id.
    pub fn children_of(&self, parent_id: Option<&str>) -> Vec<&str> {
        self.messages
            .iter()
            .filter(|m| m.parent_id.as_deref() == parent_id)
            .map(|m| m.id.as_str())
            .collect()
    }

    // ── Span helpers ──

    /// Seal the current (last) span with the given consumed IDs and summary.
    pub fn seal_current_span(&mut self, consumed_ids: &[String], summary: String) {
        if let Some(span) = self.spans.last_mut() {
            span.message_ids = consumed_ids.to_vec();
            span.summary = Some(summary);
            span.sealed_at = Some(Utc::now());
        }
    }

    /// Open a new empty span after the current one.
    pub fn open_new_span(&mut self) {
        let next_index = self.spans.last().map(|s| s.index + 1).unwrap_or(0);
        self.spans.push(Span {
            index: next_index,
            message_ids: Vec::new(),
            summary: None,
            sealed_at: None,
        });
    }

    /// Sealed span summaries in order.
    pub fn span_summaries(&self) -> Vec<&str> {
        self.spans
            .iter()
            .filter_map(|s| s.summary.as_deref())
            .collect()
    }

    /// Whether a message ID belongs to a sealed span.
    pub fn is_in_sealed_span(&self, message_id: &str) -> bool {
        self.spans
            .iter()
            .filter(|s| s.sealed_at.is_some())
            .any(|s| s.message_ids.iter().any(|id| id == message_id))
    }

    // ── API message building ──

    /// Build Anthropic API messages from span summaries + current active path.
    ///
    /// Sealed span summaries are prepended as user/assistant pairs so the model
    /// has context from previous compacted segments. Current active path messages
    /// are appended using the standard conversion logic.
    pub fn build_api_messages(&self) -> Vec<Message> {
        let mut result = Vec::new();

        // Prepend sealed span summaries as user/assistant pairs
        for summary in self.span_summaries() {
            result.push(Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: format!("[Previous conversation context]\n\n{}", summary),
                }],
            });
            result.push(Message {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "Understood, I have the previous context.".to_string(),
                }],
            });
        }

        // Append current active path messages
        result.extend(build_api_messages_from_parts(&self.active_messages()));
        result
    }
}

/// Build Anthropic API Messages from ChatMessages (raw conversion).
///
/// Handles both old format (ToolCall with inline result on assistant messages)
/// and new format (separate ToolResult parts on user messages).
/// Fences tool results and user text at the API boundary.
pub fn build_api_messages_from_parts(messages: &[&ChatMessage]) -> Vec<Message> {
    let mut result = Vec::new();

    for msg in messages {
        match msg.role {
            MessageRole::Assistant => {
                let content: Vec<ContentBlock> = msg
                    .parts
                    .iter()
                    .filter_map(|part| match part {
                        MessagePart::Text { text } => {
                            Some(ContentBlock::Text { text: text.clone() })
                        }
                        MessagePart::ToolCall {
                            tool_call_id,
                            tool_name,
                            args,
                            ..
                        } => Some(ContentBlock::ToolUse {
                            id: tool_call_id.clone(),
                            name: tool_name.clone(),
                            input: if args.is_object() {
                                args.clone()
                            } else {
                                serde_json::json!({})
                            },
                        }),
                        MessagePart::Thinking { .. } | MessagePart::ToolResult { .. } => None,
                    })
                    .collect();

                if !content.is_empty() {
                    result.push(Message {
                        role: Role::Assistant,
                        content,
                    });
                }

                // Legacy: if ToolCall parts carry inline results, emit a user
                // message with ToolResult blocks (old merged format)
                let inline_results: Vec<ContentBlock> = msg
                    .parts
                    .iter()
                    .filter_map(|part| match part {
                        MessagePart::ToolCall {
                            tool_call_id,
                            result: Some(res),
                            is_error,
                            ..
                        } => Some(ContentBlock::ToolResult {
                            tool_use_id: tool_call_id.clone(),
                            content: fence_tool_result(res),
                            is_error: Some(*is_error),
                        }),
                        _ => None,
                    })
                    .collect();

                if !inline_results.is_empty() {
                    result.push(Message {
                        role: Role::User,
                        content: inline_results,
                    });
                }
            }
            MessageRole::User => {
                let mut text_blocks = Vec::new();
                let mut tool_result_blocks = Vec::new();

                for part in &msg.parts {
                    match part {
                        MessagePart::Text { text } => {
                            text_blocks.push(ContentBlock::Text {
                                text: fence_user_message(text),
                            });
                        }
                        MessagePart::ToolResult {
                            tool_call_id,
                            result,
                            is_error,
                        } => {
                            tool_result_blocks.push(ContentBlock::ToolResult {
                                tool_use_id: tool_call_id.clone(),
                                content: fence_tool_result(result),
                                is_error: Some(*is_error),
                            });
                        }
                        _ => {}
                    }
                }

                if !tool_result_blocks.is_empty() {
                    result.push(Message {
                        role: Role::User,
                        content: tool_result_blocks,
                    });
                }
                if !text_blocks.is_empty() {
                    result.push(Message {
                        role: Role::User,
                        content: text_blocks,
                    });
                }
            }
        }
    }

    result
}

/// Where a message originated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MessageSource {
    /// Typed by the user in the UI.
    Human,
    /// Injected via MCP `send_message` or the message queue.
    Mcp,
    /// Nexus system (synthetic tool invocations, notifications, etc.)
    System {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Generated by an agent.
    Agent {
        agent_id: String,
        agent_name: String,
        model: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub parts: Vec<MessagePart>,
    pub timestamp: DateTime<Utc>,
    pub parent_id: Option<String>,
    /// Where this message came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<MessageSource>,
    /// Opaque metadata (timing spans, etc.) — passed through to the client as-is
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MessagePart {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    ToolCall {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        args: serde_json::Value,
        /// Legacy: inline result from merged format. New conversations use
        /// separate `ToolResult` parts on user messages instead.
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        #[serde(default)]
        is_error: bool,
    },
    ToolResult {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        result: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chat_msg(id: &str, role: MessageRole, parts: Vec<MessagePart>) -> ChatMessage {
        ChatMessage {
            id: id.to_string(),
            role,
            parts,
            timestamp: Utc::now(),
            parent_id: None,
            source: None,
            metadata: None,
        }
    }

    // ── build_api_messages_from_parts tests ──

    #[test]
    fn text_only_conversation() {
        let msgs = vec![
            make_chat_msg("1", MessageRole::User, vec![MessagePart::Text { text: "hi".into() }]),
            make_chat_msg("2", MessageRole::Assistant, vec![MessagePart::Text { text: "hello".into() }]),
        ];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        let api = build_api_messages_from_parts(&refs);

        assert_eq!(api.len(), 2);
        assert_eq!(api[0].role, Role::User);
        assert_eq!(api[1].role, Role::Assistant);
    }

    #[test]
    fn tool_call_new_format_produces_correct_pairing() {
        // New format: ToolCall on assistant (no inline result), ToolResult on user
        let msgs = vec![
            make_chat_msg("1", MessageRole::User, vec![MessagePart::Text { text: "do it".into() }]),
            make_chat_msg(
                "2",
                MessageRole::Assistant,
                vec![MessagePart::ToolCall {
                    tool_call_id: "tc1".into(),
                    tool_name: "bash".into(),
                    args: serde_json::json!({"command": "ls"}),
                    result: None,
                    is_error: false,
                }],
            ),
            make_chat_msg(
                "3",
                MessageRole::User,
                vec![MessagePart::ToolResult {
                    tool_call_id: "tc1".into(),
                    result: "file.txt".into(),
                    is_error: false,
                }],
            ),
        ];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        let api = build_api_messages_from_parts(&refs);

        assert_eq!(api.len(), 3);

        // [0] User text
        assert_eq!(api[0].role, Role::User);
        assert!(matches!(&api[0].content[0], ContentBlock::Text { .. }));

        // [1] Assistant tool_use
        assert_eq!(api[1].role, Role::Assistant);
        assert!(matches!(&api[1].content[0], ContentBlock::ToolUse { id, .. } if id == "tc1"));

        // [2] User tool_result
        assert_eq!(api[2].role, Role::User);
        assert!(matches!(&api[2].content[0], ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "tc1"));
    }

    #[test]
    fn tool_call_legacy_inline_result_produces_separate_user_message() {
        // Old format: ToolCall with inline result
        let msgs = vec![
            make_chat_msg("1", MessageRole::User, vec![MessagePart::Text { text: "do it".into() }]),
            make_chat_msg(
                "2",
                MessageRole::Assistant,
                vec![MessagePart::ToolCall {
                    tool_call_id: "tc1".into(),
                    tool_name: "bash".into(),
                    args: serde_json::json!({"command": "ls"}),
                    result: Some("file.txt".into()),
                    is_error: false,
                }],
            ),
        ];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        let api = build_api_messages_from_parts(&refs);

        // Should produce: User(text), Assistant(tool_use), User(tool_result from inline)
        assert_eq!(api.len(), 3);
        assert_eq!(api[0].role, Role::User);
        assert_eq!(api[1].role, Role::Assistant);
        assert!(matches!(&api[1].content[0], ContentBlock::ToolUse { .. }));
        assert_eq!(api[2].role, Role::User);
        assert!(matches!(&api[2].content[0], ContentBlock::ToolResult { .. }));
    }

    #[test]
    fn user_message_tool_results_before_text() {
        // User message with both text and tool results — results come first
        let msgs = vec![make_chat_msg(
            "1",
            MessageRole::User,
            vec![
                MessagePart::Text { text: "context".into() },
                MessagePart::ToolResult {
                    tool_call_id: "tc1".into(),
                    result: "output".into(),
                    is_error: false,
                },
            ],
        )];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        let api = build_api_messages_from_parts(&refs);

        // Should split into: User(tool_result), User(text)
        assert_eq!(api.len(), 2);
        assert!(matches!(&api[0].content[0], ContentBlock::ToolResult { .. }));
        assert!(matches!(&api[1].content[0], ContentBlock::Text { .. }));
    }

    #[test]
    fn thinking_blocks_stripped_from_api_output() {
        let msgs = vec![make_chat_msg(
            "1",
            MessageRole::Assistant,
            vec![
                MessagePart::Thinking { thinking: "hmm".into() },
                MessagePart::Text { text: "answer".into() },
            ],
        )];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        let api = build_api_messages_from_parts(&refs);

        assert_eq!(api.len(), 1);
        // Only text block, thinking stripped
        assert_eq!(api[0].content.len(), 1);
        assert!(matches!(&api[0].content[0], ContentBlock::Text { text } if text == "answer"));
    }

    #[test]
    fn empty_assistant_message_skipped() {
        let msgs = vec![make_chat_msg(
            "1",
            MessageRole::Assistant,
            vec![MessagePart::Thinking { thinking: "hmm".into() }],
        )];
        let refs: Vec<&ChatMessage> = msgs.iter().collect();
        let api = build_api_messages_from_parts(&refs);

        // Thinking-only assistant produces no content blocks, so no API message
        assert_eq!(api.len(), 0);
    }

    // ── Conversation tests ──

    #[test]
    fn active_messages_returns_ordered() {
        let conv = Conversation {
            id: "c1".into(),
            title: "test".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            messages: vec![
                make_chat_msg("a", MessageRole::User, vec![]),
                make_chat_msg("b", MessageRole::Assistant, vec![]),
                make_chat_msg("c", MessageRole::User, vec![]),
            ],
            active_path: vec!["a".into(), "c".into()], // skip "b"
            usage: None,
            agent_id: None,
            spans: vec![],
        };

        let active = conv.active_messages();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].id, "a");
        assert_eq!(active[1].id, "c");
    }

    #[test]
    fn span_summaries_returns_sealed_only() {
        let conv = Conversation {
            id: "c1".into(),
            title: "test".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            messages: vec![],
            active_path: vec![],
            usage: None,
            agent_id: None,
            spans: vec![
                Span {
                    index: 0,
                    message_ids: vec!["a".into()],
                    summary: Some("summary 1".into()),
                    sealed_at: Some(Utc::now()),
                },
                Span {
                    index: 1,
                    message_ids: vec![],
                    summary: None,
                    sealed_at: None, // open span
                },
            ],
        };

        let summaries = conv.span_summaries();
        assert_eq!(summaries, vec!["summary 1"]);
    }

    #[test]
    fn build_api_messages_includes_span_summaries() {
        let conv = Conversation {
            id: "c1".into(),
            title: "test".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            messages: vec![make_chat_msg(
                "m1",
                MessageRole::User,
                vec![MessagePart::Text { text: "latest".into() }],
            )],
            active_path: vec!["m1".into()],
            usage: None,
            agent_id: None,
            spans: vec![Span {
                index: 0,
                message_ids: vec!["old".into()],
                summary: Some("old context".into()),
                sealed_at: Some(Utc::now()),
            }],
        };

        let api = conv.build_api_messages();
        // span summary pair + active message
        assert_eq!(api.len(), 3);
        assert_eq!(api[0].role, Role::User); // summary
        assert_eq!(api[1].role, Role::Assistant); // ack
        assert_eq!(api[2].role, Role::User); // latest
    }
}
