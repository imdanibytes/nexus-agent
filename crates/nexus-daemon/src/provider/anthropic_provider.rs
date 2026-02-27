use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;

use crate::anthropic::types::{
    inject_cache_control, MessagesRequest, StreamEvent, ThinkingConfig,
};
use crate::anthropic::AnthropicClient;
use crate::provider::{InferenceProvider, InferenceRequest};

/// Beta header required for extended thinking.
const THINKING_BETA_HEADER: &str = "interleaved-thinking-2025-05-14";

pub struct AnthropicProvider {
    client: AnthropicClient,
}

impl AnthropicProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let client = if let Some(url) = base_url {
            AnthropicClient::with_base_url(api_key, url)
        } else {
            AnthropicClient::new(api_key)
        };
        Self { client }
    }
}

#[async_trait]
impl InferenceProvider for AnthropicProvider {
    async fn create_message_stream(
        &self,
        request: InferenceRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        // When thinking is enabled, temperature must be omitted (API requirement)
        let (temperature, thinking) = match request.thinking_budget {
            Some(budget) => (
                None,
                Some(ThinkingConfig {
                    thinking_type: "enabled".to_string(),
                    budget_tokens: budget,
                }),
            ),
            None => (request.temperature, None),
        };

        let has_thinking = request.thinking_budget.is_some();

        let api_request = MessagesRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens,
            system: request.system,
            messages: request.messages,
            tools: request.tools,
            stream: true,
            temperature,
            thinking,
        };

        // Serialize to JSON, inject prompt caching breakpoints, send raw
        let mut body = serde_json::to_value(&api_request)?;
        inject_cache_control(&mut body);

        // Add beta header for extended thinking
        let extra_headers = if has_thinking {
            Some(vec![("anthropic-beta", THINKING_BETA_HEADER)])
        } else {
            None
        };

        let stream = self
            .client
            .create_message_stream_json(body, extra_headers)
            .await?;
        Ok(stream.boxed())
    }
}
