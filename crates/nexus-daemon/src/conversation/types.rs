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
