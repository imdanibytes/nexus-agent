//! Post-turn auto-title generation.
//!
//! Generates or updates a conversation title using a fast, cheap LLM call.
//! Runs after the main turn completes and RUN_FINISHED is emitted — never
//! blocks the UI.

use std::sync::Arc;

use crate::agent::events::AgUiEvent;
use crate::anthropic::types::{ContentBlock, Message, MessagesRequest, Role};
use crate::anthropic::AnthropicClient;
use crate::conversation::types::{ChatMessage, MessagePart, MessageRole};
use crate::server::AppState;

const TITLE_PROMPT: &str = "\
Generate a short conversation title (3-8 words) based on the messages below.\n\
If a current title is provided and it still accurately describes the conversation topic, \
respond with exactly: KEEP\n\
If the topic has shifted or the title is generic (like 'New Chat'), \
respond with ONLY the new title — no quotes, no explanation.";

const TITLE_MODEL: &str = "claude-haiku-4-5-20251001";

/// Generate or update a conversation title and broadcast it.
///
/// Best-effort: errors are logged but never propagate. Returns the new title
/// if one was generated, or None if the current title was kept.
pub async fn generate_title(
    state: &Arc<AppState>,
    conversation_id: &str,
    current_title: &str,
    messages: &[&ChatMessage],
) -> Option<String> {
    let title_client = state.title_client.as_ref()?;

    let summary = build_summary(messages, current_title);
    if summary.is_empty() {
        return None;
    }

    match call_title_model(title_client, &summary).await {
        Ok(Some(title)) => {
            persist_and_broadcast(state, conversation_id, &title).await;
            Some(title)
        }
        Ok(None) => None, // KEEP
        Err(e) => {
            tracing::warn!("Title generation failed: {}", e);
            None
        }
    }
}

/// Build a compact summary of recent messages for the title prompt.
fn build_summary(messages: &[&ChatMessage], current_title: &str) -> String {
    let mut lines = Vec::new();

    // Include current title if non-generic so the model can decide to KEEP
    if !current_title.is_empty() && current_title != "New Chat" {
        lines.push(format!("Current title: {}", current_title));
        lines.push(String::new());
    }

    // Last 4 messages, text parts only, truncated
    for msg in messages.iter().rev().take(4).rev() {
        let role = match msg.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
        };
        for part in &msg.parts {
            if let MessagePart::Text { text } = part {
                let truncated: String = text.chars().take(300).collect();
                let suffix = if text.chars().count() > 300 { "…" } else { "" };
                lines.push(format!("{}: {}{}", role, truncated, suffix));
            }
        }
    }

    lines.join("\n")
}

/// Call the title model and parse the response.
/// Returns Ok(Some(title)) for a new title, Ok(None) for KEEP, Err on failure.
async fn call_title_model(
    client: &AnthropicClient,
    summary: &str,
) -> Result<Option<String>, String> {
    let request = MessagesRequest {
        model: TITLE_MODEL.to_string(),
        max_tokens: 30,
        system: Some(TITLE_PROMPT.to_string()),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: summary.to_string(),
            }],
        }],
        tools: Vec::new(),
        stream: false,
        temperature: None,
        thinking: None,
    };

    let response = client
        .create_message(request)
        .await
        .map_err(|e| format!("{}", e))?;

    let text = response
        .content
        .iter()
        .find_map(|block| {
            if let ContentBlock::Text { text } = block {
                Some(text.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    if text.is_empty() || text.eq_ignore_ascii_case("KEEP") {
        return Ok(None);
    }

    // Clean: strip quotes, limit length
    let cleaned = text
        .trim_matches(|c: char| c == '"' || c == '\'')
        .trim()
        .chars()
        .take(100)
        .collect::<String>();

    if cleaned.is_empty() {
        Ok(None)
    } else {
        Ok(Some(cleaned))
    }
}

/// Persist the new title and broadcast via SSE.
async fn persist_and_broadcast(state: &Arc<AppState>, conversation_id: &str, title: &str) {
    {
        let mut store = state.chat.conversations.write().await;
        if let Err(e) = store.rename(conversation_id, title) {
            tracing::error!("Failed to save title: {}", e);
        }
    }

    let _ = state
        .chat
        .event_bridge
        .agent_tx()
        .send(AgUiEvent::Custom {
            thread_id: conversation_id.to_string(),
            name: "title_update".to_string(),
            value: serde_json::json!({ "title": title }),
        });
}
