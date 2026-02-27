use anyhow::Result;

use nexus_anthropic::AnthropicClient;
use nexus_provider::types::{ContentBlock, Message, MessagesRequest, Role};

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

/// Summarize pre-built conversation text into a compact structured reference.
///
/// The caller is responsible for building the conversation text from stored
/// messages and determining which messages to consume. This function handles
/// only the LLM call and prompt construction.
pub async fn summarize_conversation(
    client: &AnthropicClient,
    conversation_text: &str,
) -> Result<String> {
    let request = MessagesRequest {
        model: SUMMARIZE_MODEL.to_string(),
        max_tokens: SUMMARIZE_MAX_TOKENS,
        system: Some(SUMMARIZE_PROMPT.to_string()),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: conversation_text.to_string(),
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

    Ok(summary_text)
}
