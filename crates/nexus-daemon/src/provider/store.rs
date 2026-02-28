use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use nexus_provider::provider_config::{Provider, ProviderType};
use crate::config::NexusConfig;

/// Parameters for creating a new provider.
pub struct CreateProviderParams {
    pub name: String,
    pub provider_type: ProviderType,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub aws_region: Option<String>,
    pub aws_profile: Option<String>,
}

pub struct ProviderStore {
    providers: Vec<Provider>,
}

impl ProviderStore {
    pub fn new(providers: Vec<Provider>) -> Self {
        Self { providers }
    }

    pub fn list(&self) -> &[Provider] {
        &self.providers
    }

    pub fn get(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn create(&mut self, params: CreateProviderParams) -> Result<Provider> {
        let now = Utc::now();
        let provider = Provider {
            id: Uuid::new_v4().to_string(),
            name: params.name,
            provider_type: params.provider_type,
            endpoint: params.endpoint,
            api_key: params.api_key,
            aws_region: params.aws_region,
            aws_profile: params.aws_profile,
            created_at: now,
            updated_at: now,
        };
        self.providers.push(provider.clone());
        self.save()?;
        Ok(provider)
    }

    pub fn update(&mut self, id: &str, updates: ProviderUpdate) -> Result<Option<Provider>> {
        let Some(provider) = self.providers.iter_mut().find(|p| p.id == id) else {
            return Ok(None);
        };

        if let Some(name) = updates.name {
            provider.name = name;
        }
        if let Some(endpoint) = updates.endpoint {
            provider.endpoint = if endpoint.is_empty() {
                None
            } else {
                Some(endpoint)
            };
        }
        if let Some(api_key) = updates.api_key {
            provider.api_key = if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            };
        }
        if let Some(region) = updates.aws_region {
            provider.aws_region = if region.is_empty() {
                None
            } else {
                Some(region)
            };
        }
        if let Some(profile) = updates.aws_profile {
            provider.aws_profile = if profile.is_empty() {
                None
            } else {
                Some(profile)
            };
        }
        provider.updated_at = Utc::now();

        let updated = provider.clone();
        self.save()?;
        Ok(Some(updated))
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let len = self.providers.len();
        self.providers.retain(|p| p.id != id);
        if self.providers.len() < len {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn save(&self) -> Result<()> {
        let mut config = NexusConfig::load()?;
        config.providers = self.providers.clone();
        config.save()
    }
}

pub struct ProviderUpdate {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub aws_region: Option<String>,
    pub aws_profile: Option<String>,
}
