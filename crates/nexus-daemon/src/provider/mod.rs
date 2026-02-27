pub mod factory;
pub mod service;
pub mod store;

// Re-export sub-modules so existing `crate::provider::types::*` and
// `crate::provider::error::*` paths continue working.
pub mod types {
    pub use nexus_provider::provider_config::*;
}
pub mod error {
    pub use nexus_provider::error::*;
}

pub use nexus_provider::{InferenceProvider, InferenceRequest};
pub use types::{Provider, ProviderPublic, ProviderType};

pub use service::ProviderService;
pub use store::ProviderStore;
