// Re-export the entire module system from nexus-core.
// The daemon uses these types throughout — this facade keeps `crate::module::*` working.
pub use nexus_core::*;

// ── Bridge: anthropic types ↔ nexus-core types ──
//
// StopReason is a foreign type (defined in nexus-core and nexus-provider
// respectively), so we can't impl From due to the orphan rule.

use nexus_provider::types as api;

pub fn stop_reason_from_api(sr: &api::StopReason) -> StopReason {
    match sr {
        api::StopReason::EndTurn => StopReason::EndTurn,
        api::StopReason::MaxTokens => StopReason::MaxTokens,
        api::StopReason::StopSequence => StopReason::StopSequence,
        api::StopReason::ToolUse => StopReason::ToolUse,
    }
}
