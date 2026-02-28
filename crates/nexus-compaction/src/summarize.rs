use anyhow::Result;
use futures::StreamExt;

use nexus_provider::types::{ContentBlock, Delta, Message, Role, StreamEvent};
use nexus_provider::{InferenceProvider, InferenceRequest};

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

/// Result of a summarization call, including token usage for cost tracking.
pub struct SummarizeResult {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Summarize pre-built conversation text into a compact structured reference.
///
/// The caller is responsible for building the conversation text from stored
/// messages and determining which messages to consume. This function handles
/// only the LLM call and prompt construction.
pub async fn summarize_conversation(
    provider: &dyn InferenceProvider,
    model: &str,
    conversation_text: &str,
) -> Result<SummarizeResult> {
    let messages = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: conversation_text.to_string(),
        }],
    }];

    let mut stream = provider
        .create_message_stream(InferenceRequest {
            model: model.to_string(),
            max_tokens: SUMMARIZE_MAX_TOKENS,
            system: Some(SUMMARIZE_PROMPT.to_string()),
            temperature: Some(0.0),
            thinking_budget: None,
            messages,
            tools: Vec::new(),
        })
        .await
        .map_err(|e| anyhow::anyhow!("stream creation failed: {}", e))?;

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
                return Err(anyhow::anyhow!("stream error: {}", message));
            }
            Err(e) => {
                return Err(anyhow::anyhow!("stream error: {}", e));
            }
            _ => {}
        }
    }

    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(SummarizeResult {
            text: "[Compaction summary unavailable]".to_string(),
            input_tokens,
            output_tokens,
        });
    }

    Ok(SummarizeResult {
        text,
        input_tokens,
        output_tokens,
    })
}
