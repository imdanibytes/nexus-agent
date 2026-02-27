// Re-export the entire module system from nexus-core.
// The daemon uses these types throughout — this facade keeps `crate::module::*` working.
pub use nexus_core::*;

// ── Bridge: anthropic types ↔ nexus-core types ──
//
// Both StopReason and ToolDefinition/Tool are now foreign types (defined in
// nexus-core and nexus-provider respectively), so we can't impl From due to
// the orphan rule. Standalone conversion functions instead.

use crate::anthropic::types as api;

pub fn stop_reason_from_api(sr: &api::StopReason) -> StopReason {
    match sr {
        api::StopReason::EndTurn => StopReason::EndTurn,
        api::StopReason::MaxTokens => StopReason::MaxTokens,
        api::StopReason::StopSequence => StopReason::StopSequence,
        api::StopReason::ToolUse => StopReason::ToolUse,
    }
}

pub fn tool_from_definition(td: ToolDefinition) -> api::Tool {
    api::Tool {
        name: td.name,
        description: td.description,
        input_schema: td.input_schema,
    }
}
