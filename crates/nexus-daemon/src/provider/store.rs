use anyhow::Result;
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use super::types::{Provider, ProviderType};

pub struct ProviderStore {
    path: PathBuf,
    providers: Vec<Provider>,
}

impl ProviderStore {
    pub fn load(nexus_dir: &std::path::Path) -> Result<Self> {
        let path = nexus_dir.join("providers.json");
        let providers = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            Vec::new()
        };
        Ok(Self { path, providers })
    }

    pub fn list(&self) -> &[Provider] {
        &self.providers
    }

    pub fn get(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn create(
        &mut self,
        name: String,
        provider_type: ProviderType,
        endpoint: Option<String>,
        api_key: Option<String>,
        aws_region: Option<String>,
        aws_access_key_id: Option<String>,
        aws_secret_access_key: Option<String>,
        aws_session_token: Option<String>,
    ) -> Result<Provider> {
        let now = Utc::now();
        let provider = Provider {
            id: Uuid::new_v4().to_string(),
            name,
            provider_type,
            endpoint,
            api_key,
            aws_region,
            aws_access_key_id,
            aws_secret_access_key,
            aws_session_token,
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
        if let Some(key) = updates.aws_access_key_id {
            provider.aws_access_key_id = if key.is_empty() { None } else { Some(key) };
        }
        if let Some(secret) = updates.aws_secret_access_key {
            provider.aws_secret_access_key = if secret.is_empty() {
                None
            } else {
                Some(secret)
            };
        }
        if let Some(token) = updates.aws_session_token {
            provider.aws_session_token = if token.is_empty() {
                None
            } else {
                Some(token)
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
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.providers)?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}

pub struct ProviderUpdate {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub aws_region: Option<String>,
    pub aws_access_key_id: Option<String>,
    pub aws_secret_access_key: Option<String>,
    pub aws_session_token: Option<String>,
}
