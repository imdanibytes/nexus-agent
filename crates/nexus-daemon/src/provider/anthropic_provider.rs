use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;

use crate::anthropic::types::{Message, MessagesRequest, StreamEvent, Tool};
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
        let mut request = MessagesRequest {
            model: model.to_string(),
            max_tokens,
            system,
            messages,
            tools,
            stream: true,
            temperature: None,
        };
        if let Some(t) = temperature {
            request.temperature = Some(t);
        }

        let stream = self.client.create_message_stream(request).await?;
        Ok(stream.boxed())
    }
}
