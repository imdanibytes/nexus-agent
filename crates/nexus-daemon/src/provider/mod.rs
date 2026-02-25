pub mod anthropic_provider;
pub mod bedrock_provider;
pub mod error;
pub mod factory;
pub mod store;
pub mod types;

pub use factory::ProviderFactory;
pub use store::ProviderStore;
pub use types::{Provider, ProviderPublic, ProviderType};

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::anthropic::types::{Message, StreamEvent, Tool};

/// Abstraction over LLM providers (Anthropic, Bedrock, etc.)
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    async fn create_message_stream(
        &self,
        model: &str,
        max_tokens: u32,
        system: Option<String>,
        temperature: Option<f32>,
        thinking_budget: Option<u32>,
        messages: Vec<Message>,
        tools: Vec<Tool>,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>>;
}
