use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::anthropic_provider::AnthropicProvider;
use super::bedrock_provider::BedrockProvider;
use super::types::{Provider, ProviderType};
use super::InferenceProvider;

type ProviderCache = HashMap<String, (DateTime<Utc>, Arc<dyn InferenceProvider>)>;

pub struct ProviderFactory {
    cache: RwLock<ProviderCache>,
}

impl ProviderFactory {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get(&self, provider: &Provider) -> Result<Arc<dyn InferenceProvider>> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some((cached_at, instance)) = cache.get(&provider.id) {
                if *cached_at == provider.updated_at {
                    return Ok(Arc::clone(instance));
                }
            }
        }

        // Build new instance
        let instance: Arc<dyn InferenceProvider> = match provider.provider_type {
            ProviderType::Anthropic => {
                let api_key = provider
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow!("Anthropic provider '{}' has no API key", provider.name))?
                    .clone();
                Arc::new(AnthropicProvider::new(api_key, provider.endpoint.clone()))
            }
            ProviderType::Bedrock => {
                let region = provider
                    .aws_region
                    .as_ref()
                    .ok_or_else(|| anyhow!("Bedrock provider '{}' has no AWS region", provider.name))?
                    .clone();
                let bedrock =
                    BedrockProvider::new(&region, provider.aws_profile.as_deref()).await?;
                Arc::new(bedrock)
            }
        };

        // Cache it
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                provider.id.clone(),
                (provider.updated_at, Arc::clone(&instance)),
            );
        }

        Ok(instance)
    }

    pub async fn invalidate(&self, provider_id: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(provider_id);
    }
}
