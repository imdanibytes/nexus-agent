use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::memory::embedding::EmbeddingProvider;
use crate::memory::error::MemoryError;

/// Ollama embedding provider. Hits the `/api/embed` endpoint which
/// supports native batching and returns L2-normalized vectors.
pub struct OllamaEmbedder {
    client: reqwest::Client,
    base_url: String,
    model: String,
    dimensions: usize,
}

impl OllamaEmbedder {
    /// Create a new Ollama embedder.
    ///
    /// `base_url` is typically `http://localhost:11434`.
    /// `model` is the embedding model name (e.g. `nomic-embed-text`, `mxbai-embed-large`).
    /// `dimensions` must match the model's output dimensionality.
    pub fn new(base_url: &str, model: &str, dimensions: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            dimensions,
        }
    }
}

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError> {
        let mut results = self.embed_batch(&[text.to_string()]).await?;
        results
            .pop()
            .ok_or_else(|| MemoryError::Embedding("empty response from ollama".into()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, MemoryError> {
        let url = format!("{}/api/embed", self.base_url);

        let body = EmbedRequest {
            model: self.model.clone(),
            input: texts.to_vec(),
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| MemoryError::Embedding(format!("request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MemoryError::Embedding(format!(
                "ollama returned {status}: {body}"
            )));
        }

        let parsed: EmbedResponse = response
            .json()
            .await
            .map_err(|e| MemoryError::Embedding(format!("failed to parse response: {e}")))?;

        if parsed.embeddings.len() != texts.len() {
            return Err(MemoryError::Embedding(format!(
                "expected {} embeddings, got {}",
                texts.len(),
                parsed.embeddings.len()
            )));
        }

        Ok(parsed.embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}
