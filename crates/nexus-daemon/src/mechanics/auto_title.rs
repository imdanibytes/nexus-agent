//! Post-turn auto-title generation.
//!
//! Generates or updates a conversation title using a fast, cheap LLM call.
//! Runs after the main turn completes and RUN_FINISHED is emitted — never
//! blocks the UI.

use std::sync::Arc;

use futures::StreamExt;

use crate::agent::events::AgUiEvent;
use crate::anthropic::types::{
    ContentBlock, Delta, Message, Role, StreamEvent,
};
use crate::conversation::types::{ChatMessage, MessagePart, MessageRole};
use crate::provider::InferenceProvider;
use crate::server::AppState;

const TITLE_PROMPT: &str = "\
Generate a short conversation title (3-8 words) based on the messages below.\n\
If a current title is provided and it still accurately describes the conversation topic, \
respond with exactly: KEEP\n\
If the topic has shifted or the title is generic (like 'New Chat'), \
respond with ONLY the new title — no quotes, no explanation.";

/// Title model per provider type. Bedrock uses the cross-region inference
/// profile ID; direct Anthropic uses the standard model name.
const TITLE_MODEL_ANTHROPIC: &str = "claude-haiku-4-5-20251001";
const TITLE_MODEL_BEDROCK: &str = "us.anthropic.claude-3-haiku-20240307-v1:0";

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
    let summary = build_summary(messages, current_title);
    if summary.is_empty() {
        tracing::debug!("auto_title: empty summary, skipping");
        return None;
    }

    // Resolve the provider via the factory (works for Anthropic, Bedrock, etc.)
    let (provider, title_model) = resolve_provider(state).await?;

    match call_title_model(provider.as_ref(), &title_model, &summary).await {
        Ok(Some(title)) => {
            tracing::info!("auto_title: generated title '{}' for conv={}", title, conversation_id);
            persist_and_broadcast(state, conversation_id, &title).await;
            Some(title)
        }
        Ok(None) => {
            tracing::debug!("auto_title: model said KEEP for conv={}", conversation_id);
            None
        }
        Err(e) => {
            tracing::warn!("auto_title: title generation failed: {}", e);
            None
        }
    }
}

/// Resolve an InferenceProvider from the active agent's provider config.
/// Returns the provider instance and the appropriate title model name.
async fn resolve_provider(
    state: &Arc<AppState>,
) -> Option<(Arc<dyn InferenceProvider>, String)> {
    let agents = state.agents.agents.read().await;
    let providers = state.agents.providers.read().await;

    let active_id = agents.active_agent_id()?;
    let agent = agents.get(active_id)?;
    let provider_record = providers.get(&agent.provider_id)?.clone();

    // Pick the right title model for the provider type
    let title_model = match provider_record.provider_type {
        crate::provider::types::ProviderType::Anthropic => TITLE_MODEL_ANTHROPIC.to_string(),
        crate::provider::types::ProviderType::Bedrock => TITLE_MODEL_BEDROCK.to_string(),
    };

    // Drop read locks before the potentially-async factory call
    drop(agents);
    drop(providers);

    match state.agents.factory.get(&provider_record).await {
        Ok(instance) => Some((instance, title_model)),
        Err(e) => {
            tracing::warn!("auto_title: failed to create provider: {}", e);
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

/// Call the title model via streaming and collect the response text.
/// Returns Ok(Some(title)) for a new title, Ok(None) for KEEP, Err on failure.
async fn call_title_model(
    provider: &dyn InferenceProvider,
    model: &str,
    summary: &str,
) -> Result<Option<String>, String> {
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: summary.to_string(),
        }],
    }];

    let mut stream = provider
        .create_message_stream(
            model,
            30, // max_tokens — titles are short
            Some(TITLE_PROMPT.to_string()),
            None, // temperature
            None, // thinking_budget
            messages,
            Vec::new(), // no tools
        )
        .await
        .map_err(|e| format!("stream creation failed: {}", e))?;

    // Collect text deltas from the stream
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::ContentBlockDelta {
                delta: Delta::TextDelta { text: chunk },
                ..
            }) => {
                text.push_str(&chunk);
            }
            Ok(StreamEvent::MessageStop) => break,
            Ok(StreamEvent::Error { message, .. }) => {
                return Err(format!("stream error: {}", message));
            }
            Err(e) => {
                return Err(format!("stream error: {}", e));
            }
            _ => {} // skip other events
        }
    }

    let text = text.trim().to_string();

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
