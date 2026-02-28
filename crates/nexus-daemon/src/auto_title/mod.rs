//! Post-turn auto-title generation as a DaemonModule.
//!
//! Generates or updates a conversation title using a fast, cheap LLM call.
//! Runs via the `turn_end` hook after the main turn completes — never blocks
//! the UI.

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;

use nexus_provider::types::{ContentBlock, Delta, Message, Role, StreamEvent};
use crate::agent_config::AgentService;
use crate::config::{ModelTier, ModelTierConfig};
use crate::conversation::types::{ChatMessage, MessagePart, MessageRole};
use crate::module::{DaemonModule, DoctorCheck, DoctorReport, DoctorStatus, TurnEndEvent};
use nexus_provider::{InferenceProvider, InferenceRequest};
use crate::provider::ProviderService;
use crate::thread::ThreadService;

const TITLE_PROMPT: &str = "\
Generate a short conversation title (3-8 words) based on the messages below.\n\
If a current title is provided and it still accurately describes the conversation topic, \
respond with exactly: KEEP\n\
If the topic has shifted or the title is generic (like 'New Chat'), \
respond with ONLY the new title — no quotes, no explanation.";

/// Result of a title generation call, including cost information.
struct TitleResult {
    /// The new title, or None if KEEP.
    title: Option<String>,
    /// Cost of the title generation call in USD.
    cost: f64,
}

pub struct AutoTitleModule {
    pub threads: Arc<ThreadService>,
    pub agents: Arc<AgentService>,
    pub providers: Arc<ProviderService>,
    pub model_tiers: ModelTierConfig,
}

#[async_trait]
impl DaemonModule for AutoTitleModule {
    fn name(&self) -> &str {
        "auto_title"
    }

    async fn turn_end(&self, event: &TurnEndEvent<'_>) {
        if event.error.is_some() {
            return;
        }

        let conv = match self.threads.get(event.conversation_id).await.ok().flatten() {
            Some(c) => c,
            None => return,
        };

        let active = conv.active_messages();
        let summary = build_summary(&active, &conv.title);
        if summary.is_empty() {
            tracing::debug!("auto_title: empty summary, skipping");
            return;
        }

        let (provider, title_model) = match self.resolve_provider().await {
            Some(p) => p,
            None => return,
        };

        match call_title_model(provider.as_ref(), &title_model, &summary).await {
            Ok(result) => {
                if result.cost > 0.0 {
                    if let Err(e) = self.threads.add_cost(event.conversation_id, result.cost).await {
                        tracing::error!("auto_title: failed to save cost: {}", e);
                    }
                    tracing::debug!(
                        "auto_title: added ${:.6} to conv={}",
                        result.cost,
                        event.conversation_id
                    );
                }

                if let Some(ref title) = result.title {
                    tracing::info!(
                        "auto_title: generated title '{}' for conv={} (cost=${:.6})",
                        title,
                        event.conversation_id,
                        result.cost
                    );
                    if let Err(e) = self.threads.rename(event.conversation_id, title).await {
                        tracing::error!("auto_title: failed to save title: {}", e);
                    }
                } else {
                    tracing::debug!(
                        "auto_title: model said KEEP for conv={}",
                        event.conversation_id
                    );
                }
            }
            Err(e) => {
                tracing::warn!("auto_title: title generation failed: {}", e);
            }
        }
    }

    async fn doctor(&self) -> DoctorReport {
        let has_provider = self.resolve_provider().await.is_some();
        DoctorReport {
            module: "auto_title".into(),
            status: if has_provider {
                DoctorStatus::Healthy
            } else {
                DoctorStatus::Degraded
            },
            checks: vec![DoctorCheck {
                name: "title_provider_available".into(),
                passed: has_provider,
                message: if has_provider {
                    "Title model provider is available".into()
                } else {
                    "No active agent/provider — titles won't be generated".into()
                },
            }],
        }
    }
}

impl AutoTitleModule {
    /// Resolve an InferenceProvider from the active agent's provider config.
    async fn resolve_provider(&self) -> Option<(Arc<dyn InferenceProvider>, String)> {
        let agent = self.agents.active_agent().await?;
        let provider_record = self.providers.get(&agent.provider_id).await?;

        let title_model = self.model_tiers.resolve(
            &provider_record.provider_type,
            ModelTier::Fast,
        );

        match self.providers.get_client(&provider_record).await {
            Ok(instance) => Some((instance, title_model)),
            Err(e) => {
                tracing::warn!("auto_title: failed to create provider: {}", e);
                None
            }
        }
    }
}

/// Build a compact summary of recent messages for the title prompt.
fn build_summary(messages: &[&ChatMessage], current_title: &str) -> String {
    let mut lines = Vec::new();

    if !current_title.is_empty() && current_title != "New Chat" {
        lines.push(format!("Current title: {}", current_title));
        lines.push(String::new());
    }

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
        .create_message_stream(InferenceRequest {
            model: model.to_string(),
            max_tokens: 30,
            system: Some(TITLE_PROMPT.to_string()),
            temperature: None,
            thinking_budget: None,
            messages,
            tools: Vec::new(),
        })
        .await
        .map_err(|e| format!("stream creation failed: {}", e))?;

    let mut text = String::new();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::MessageStart {
                usage: Some(ref usage),
                ..
            }) => {
                input_tokens = usage.input_tokens;
            }
            Ok(StreamEvent::ContentBlockDelta {
                delta: Delta::TextDelta { text: chunk },
                ..
            }) => {
                text.push_str(&chunk);
            }
            Ok(StreamEvent::MessageDelta {
                usage: Some(ref u), ..
            }) => {
                output_tokens = u.output_tokens;
            }
            Ok(StreamEvent::MessageStop) => break,
            Ok(StreamEvent::Error { message, .. }) => {
                return Err(format!("stream error: {}", message));
            }
            Err(e) => {
                return Err(format!("stream error: {}", e));
            }
            _ => {}
        }
    }

    let cost = nexus_pricing::calculate_cost(model, input_tokens, output_tokens);
    let text = text.trim().to_string();

    if text.is_empty() || text.eq_ignore_ascii_case("KEEP") {
        return Ok(TitleResult { title: None, cost });
    }

    let cleaned = text
        .trim_matches(|c: char| c == '"' || c == '\'')
        .trim()
        .chars()
        .take(100)
        .collect::<String>();

    if cleaned.is_empty() {
        Ok(TitleResult { title: None, cost })
    } else {
        Ok(TitleResult {
            title: Some(cleaned),
            cost,
        })
    }
}
