pub mod anthropic;
pub mod ollama;
pub mod openai;

use async_trait::async_trait;

use crate::error::InferenceError;
use crate::types::{InferenceRequest, InferenceResponse};

/// Pure LLM API call. No state, no history, no context management.
/// Request in, response out.
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse, InferenceError>;
}

/// Blanket impl so `Box<dyn InferenceProvider>` can be passed directly to `Agent::new()`.
#[async_trait]
impl InferenceProvider for Box<dyn InferenceProvider> {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse, InferenceError> {
        (**self).infer(request).await
    }
}

pub use anthropic::AnthropicProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
