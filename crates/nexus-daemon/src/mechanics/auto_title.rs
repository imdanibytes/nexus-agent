//! Post-turn auto-title generation.
//!
//! Generates or updates a conversation title using a fast, cheap LLM call.
//! Runs after the main turn completes and RUN_FINISHED is emitted — never
//! blocks the UI.

use std::sync::Arc;

use futures::StreamExt;

use crate::agent::events::{AgUiEvent, EventEnvelope};
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
const TITLE_MODEL_BEDROCK: &str = "us.anthropic.claude-haiku-4-5-20251001-v1:0";

/// Result of a title generation call, including cost information.
struct TitleResult {
    /// The new title, or None if KEEP.
    title: Option<String>,
    /// Cost of the title generation call in USD.
    cost: f64,
}

/// Generate or update a conversation title and broadcast it.
///
/// Best-effort: errors are logged but never propagate. Returns the new title
/// if one was generated, or None if the current title was kept.
/// The cost of the title generation call is always added to the conversation.
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
        Ok(result) => {
            // Always add the cost to the conversation, even for KEEP
            if result.cost > 0.0 {
                add_cost_to_conversation(state, conversation_id, result.cost).await;
                tracing::debug!("auto_title: added ${:.6} to conv={}", result.cost, conversation_id);
            }

            if let Some(ref title) = result.title {
                tracing::info!("auto_title: generated title '{}' for conv={} (cost=${:.6})", title, conversation_id, result.cost);
                persist_and_broadcast(state, conversation_id, title).await;
            } else {
                tracing::debug!("auto_title: model said KEEP for conv={}", conversation_id);
            }
            result.title
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

/// Call the title model via streaming and collect the response text + usage.
/// Returns Ok(TitleResult) with title and cost, Err on failure.
async fn call_title_model(
    provider: &dyn InferenceProvider,
    model: &str,
    summary: &str,
) -> Result<TitleResult, String> {
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

    // Collect text deltas and usage from the stream
    let mut text = String::new();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::MessageStart { usage, .. }) => {
                // Capture input token count from the message start event
                if let Some(usage) = &usage {
                    input_tokens = usage.input_tokens;
                }
            }
            Ok(StreamEvent::ContentBlockDelta {
                delta: Delta::TextDelta { text: chunk },
                ..
            }) => {
                text.push_str(&chunk);
            }
            Ok(StreamEvent::MessageDelta { usage, .. }) => {
                // Capture output token count from the message delta event
                if let Some(u) = &usage {
                    output_tokens = u.output_tokens;
                }
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

    let cost = crate::pricing::calculate_cost(model, input_tokens, output_tokens);

    let text = text.trim().to_string();

    if text.is_empty() || text.eq_ignore_ascii_case("KEEP") {
        return Ok(TitleResult { title: None, cost });
    }

    // Clean: strip quotes, limit length
    let cleaned = text
        .trim_matches(|c: char| c == '"' || c == '\'')
        .trim()
        .chars()
        .take(100)
        .collect::<String>();

    if cleaned.is_empty() {
        Ok(TitleResult { title: None, cost })
    } else {
        Ok(TitleResult { title: Some(cleaned), cost })
    }
}

/// Add the title generation cost to the conversation's running total.
async fn add_cost_to_conversation(state: &Arc<AppState>, conversation_id: &str, cost: f64) {
    if let Err(e) = state.threads.add_cost(conversation_id, cost).await {
        tracing::error!("auto_title: failed to save cost: {}", e);
    }
}

/// Persist the new title and broadcast via SSE.
async fn persist_and_broadcast(state: &Arc<AppState>, conversation_id: &str, title: &str) {
    if let Err(e) = state.threads.rename(conversation_id, title).await {
        tracing::error!("Failed to save title: {}", e);
    }

    let _ = state
        .chat
        .event_bridge
        .agent_tx()
        .send(EventEnvelope {
            thread_id: Some(conversation_id.to_string()),
            run_id: None,
            event: AgUiEvent::Custom {
                name: "title_update".to_string(),
                value: serde_json::json!({ "title": title }),
            },
        });
}
