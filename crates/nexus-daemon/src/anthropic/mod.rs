// Re-export shared types from nexus-provider so all existing
// `crate::anthropic::types::*` imports continue working unchanged.
pub mod types {
    pub use nexus_provider::types::*;
}

pub use nexus_anthropic::AnthropicClient;
