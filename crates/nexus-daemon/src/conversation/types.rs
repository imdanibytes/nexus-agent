use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub context_window: u32,
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

    /// Normalize legacy format: merge tool results from user messages into
    /// the preceding assistant message's ToolCall parts, then drop those
    /// tool-result-only user messages.  The Anthropic API uses separate user
    /// messages for tool results, but our storage/display format keeps tool
    /// calls self-contained on assistant messages (AG-UI style).
    pub fn normalize_tool_calls(&mut self) {
        let mut i = 0;
        while i < self.messages.len() {
            if self.messages[i].role != MessageRole::User {
                i += 1;
                continue;
            }

            // Collect tool results from this user message
            let tool_results: Vec<(String, String, bool)> = self.messages[i]
                .parts
                .iter()
                .filter_map(|p| match p {
                    MessagePart::ToolCall {
                        tool_call_id,
                        result: Some(res),
                        is_error,
                        ..
                    } => Some((tool_call_id.clone(), res.clone(), *is_error)),
                    _ => None,
                })
                .collect();

            if tool_results.is_empty() {
                i += 1;
                continue;
            }

            // Merge into preceding assistant message
            if let Some(asst) = self.messages[..i]
                .iter_mut()
                .rev()
                .find(|m| m.role == MessageRole::Assistant)
            {
                for (tc_id, res, is_err) in &tool_results {
                    for part in asst.parts.iter_mut() {
                        if let MessagePart::ToolCall {
                            tool_call_id,
                            result: ref mut slot,
                            is_error: ref mut err_slot,
                            ..
                        } = part
                        {
                            if tool_call_id == tc_id && slot.is_none() {
                                *slot = Some(res.clone());
                                *err_slot = *is_err;
                            }
                        }
                    }
                }
            }

            // Remove if user message has no text
            let has_text = self.messages[i]
                .parts
                .iter()
                .any(|p| matches!(p, MessagePart::Text { text } if !text.is_empty()));

            if !has_text {
                let removed_id = self.messages[i].id.clone();
                let removed_parent = self.messages[i].parent_id.clone();
                self.messages.remove(i);

                // Fix parent_id chain + active_path
                for m in self.messages.iter_mut() {
                    if m.parent_id.as_deref() == Some(&removed_id) {
                        m.parent_id = removed_parent.clone();
                    }
                }
                self.active_path.retain(|id| id != &removed_id);
                continue;
            }

            i += 1;
        }
    }

    /// Compute branch_info: maps parent_id → list of child message IDs.
    pub fn branch_info(&self) -> HashMap<String, Vec<String>> {
        let mut info: HashMap<String, Vec<String>> = HashMap::new();
        for msg in &self.messages {
            let key = msg.parent_id.clone().unwrap_or_default();
            info.entry(key).or_default().push(msg.id.clone());
        }
        // Only keep entries with more than one child (actual branch points)
        info.retain(|_, children| children.len() > 1);
        info
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub parts: Vec<MessagePart>,
    pub timestamp: DateTime<Utc>,
    pub parent_id: Option<String>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        #[serde(default)]
        is_error: bool,
    },
}
