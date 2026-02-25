use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;

use crate::anthropic::types::{inject_cache_control, Message, MessagesRequest, StreamEvent, Tool};
use crate::anthropic::AnthropicClient;
use crate::provider::InferenceProvider;

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
        model: &str,
        max_tokens: u32,
        system: Option<String>,
        temperature: Option<f32>,
        messages: Vec<Message>,
        tools: Vec<Tool>,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let request = MessagesRequest {
            model: model.to_string(),
            max_tokens,
            system,
            messages,
            tools,
            stream: true,
            temperature,
        };

        // Serialize to JSON, inject prompt caching breakpoints, send raw
        let mut body = serde_json::to_value(&request)?;
        inject_cache_control(&mut body);

        let stream = self.client.create_message_stream_json(body).await?;
        Ok(stream.boxed())
    }
}
