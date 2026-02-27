pub mod error;
pub mod provider_config;
pub mod types;

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;

use types::{Message, StreamEvent, Tool};

/// Parameters for an inference request to an LLM provider.
pub struct InferenceRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub thinking_budget: Option<u32>,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
}

/// Abstraction over LLM providers (Anthropic, Bedrock, etc.)
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    async fn create_message_stream(
        &self,
        request: InferenceRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>>;
}
