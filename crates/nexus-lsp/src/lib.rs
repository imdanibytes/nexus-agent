pub mod config;
pub mod detect;
pub mod diagnostics;
pub mod languages;
pub mod manager;
pub mod protocol;
pub mod server;

use std::sync::Arc;
use tokio::sync::RwLock;

use config::LspConfigStore;
use manager::LspManager;

/// Top-level LSP service.
pub struct LspService {
    pub manager: RwLock<LspManager>,
    pub configs: RwLock<LspConfigStore>,
}

impl LspService {
    pub fn new(manager: LspManager, configs: LspConfigStore) -> Arc<Self> {
        Arc::new(Self {
            manager: RwLock::new(manager),
            configs: RwLock::new(configs),
        })
    }
}
