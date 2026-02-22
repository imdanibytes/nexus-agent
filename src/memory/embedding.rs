use async_trait::async_trait;

use super::error::MemoryError;

/// Embedding provider trait. OpenAI, local model, Anthropic, whatever.
/// Implement this to plug in your embedding backend.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>;

    /// Embed a batch of texts. Default implementation calls `embed` in sequence.
    /// Override for providers that support native batching.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MemoryError> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Dimensionality of the embedding vectors this provider produces.
    fn dimensions(&self) -> usize;
}
