use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::factory::ProviderFactory;
use super::store::{CreateProviderParams, ProviderStore, ProviderUpdate};
use nexus_provider::provider_config::Provider;
use nexus_provider::InferenceProvider;
use crate::event_bus::EventBus;

/// Encapsulated provider management service.
///
/// Wraps `ProviderStore` + `ProviderFactory` with internal locking,
/// method-based access, auto-invalidation on mutations, and EventBus emission.
pub struct ProviderService {
    store: RwLock<ProviderStore>,
    factory: ProviderFactory,
    event_bus: EventBus,
}

impl ProviderService {
    pub fn new(store: ProviderStore, event_bus: EventBus) -> Self {
        Self {
            store: RwLock::new(store),
            factory: ProviderFactory::new(),
            event_bus,
        }
    }

    // -- Reads ----------------------------------------------------------------

    pub async fn list(&self) -> Vec<Provider> {
        self.store.read().await.list().to_vec()
    }

    pub async fn get(&self, id: &str) -> Option<Provider> {
        self.store.read().await.get(id).cloned()
    }

    pub async fn exists(&self, id: &str) -> bool {
        self.store.read().await.get(id).is_some()
    }

    /// Get a cached inference client for a provider record.
    ///
    /// Reuses cached clients when the provider hasn't been updated.
    /// Pass a full `Provider` record — useful for both stored and temporary
    /// (inline-test) providers.
    pub async fn get_client(&self, provider: &Provider) -> Result<Arc<dyn InferenceProvider>> {
        self.factory.get(provider).await
    }

    /// Look up a provider by ID and return a cached inference client.
    #[allow(dead_code)] // part of service API
    pub async fn get_client_by_id(&self, id: &str) -> Result<Option<Arc<dyn InferenceProvider>>> {
        let provider = {
            let store = self.store.read().await;
            match store.get(id) {
                Some(p) => p.clone(),
                None => return Ok(None),
            }
        };
        let client = self.factory.get(&provider).await?;
        Ok(Some(client))
    }

    // -- Writes ---------------------------------------------------------------

    pub async fn create(&self, params: CreateProviderParams) -> Result<Provider> {
        let provider = self.store.write().await.create(params)?;
        self.event_bus.emit_global("provider_created", serde_json::to_value(&provider).unwrap_or_default());
        Ok(provider)
    }

    pub async fn update(&self, id: &str, updates: ProviderUpdate) -> Result<Option<Provider>> {
        let result = self.store.write().await.update(id, updates)?;
        if let Some(ref provider) = result {
            // Invalidate cached client so the next call rebuilds with new config
            self.factory.invalidate(id).await;
            self.event_bus.emit_global("provider_updated", serde_json::to_value(provider).unwrap_or_default());
        }
        Ok(result)
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        let deleted = self.store.write().await.delete(id)?;
        if deleted {
            self.factory.invalidate(id).await;
            self.event_bus.emit_global("provider_deleted", serde_json::json!({ "id": id }));
        }
        Ok(deleted)
    }
}
