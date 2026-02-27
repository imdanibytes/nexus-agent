//! Debug-only hook probe module for integration testing.
//!
//! Records every hook invocation to a shared Vec and supports configurable
//! behavior (deny tools, force continuation). Only compiled in debug builds.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use crate::module::*;

#[derive(Clone, Debug, serde::Serialize)]
pub struct HookRecord {
    pub hook: String,
    pub conversation_id: String,
    pub timestamp_ms: u64,
    pub details: serde_json::Value,
}

pub struct HookProbeState {
    pub records: Vec<HookRecord>,
    pub deny_tools: HashSet<String>,
}

pub struct HookProbe {
    state: Mutex<HookProbeState>,
    force_continue_count: AtomicU32,
}

impl HookProbe {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HookProbeState {
                records: Vec::new(),
                deny_tools: HashSet::new(),
            }),
            force_continue_count: AtomicU32::new(0),
        }
    }

    fn record(&self, hook: &str, conversation_id: &str, details: serde_json::Value) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let mut state = self.state.lock().unwrap();
        state.records.push(HookRecord {
            hook: hook.to_string(),
            conversation_id: conversation_id.to_string(),
            timestamp_ms,
            details,
        });
    }

    pub fn get_records(&self) -> Vec<HookRecord> {
        self.state.lock().unwrap().records.clone()
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.records.clear();
        state.deny_tools.clear();
        self.force_continue_count.store(0, Ordering::Relaxed);
    }

    pub fn deny_tool(&self, tool_name: String) {
        self.state.lock().unwrap().deny_tools.insert(tool_name);
    }

    pub fn set_force_continue(&self, count: u32) {
        self.force_continue_count.store(count, Ordering::Relaxed);
    }
}

#[async_trait]
impl DaemonModule for HookProbe {
    fn name(&self) -> &str {
        "hook_probe"
    }

    async fn turn_start(&self, event: &TurnStartEvent<'_>) {
        self.record("turn_start", event.conversation_id, serde_json::json!({
            "run_id": event.run_id,
            "depth": event.depth,
        }));
    }

    async fn pre_tool_use(&self, event: &PreToolUseEvent<'_>) -> PreToolUseDecision {
        let deny = self.state.lock().unwrap().deny_tools.contains(event.tool_name);
        self.record("pre_tool_use", event.conversation_id, serde_json::json!({
            "tool_name": event.tool_name,
            "denied": deny,
        }));
        if deny {
            PreToolUseDecision::Deny(format!("HookProbe denied tool: {}", event.tool_name))
        } else {
            PreToolUseDecision::Allow
        }
    }

    async fn post_tool_use(&self, event: &mut PostToolUseEvent<'_>) {
        self.record("post_tool_use", event.conversation_id, serde_json::json!({
            "tool_name": event.tool_name,
            "is_error": event.result.is_error,
        }));
    }

    async fn post_tool_use_failure(&self, event: &PostToolUseFailureEvent<'_>) {
        self.record("post_tool_use_failure", event.conversation_id, serde_json::json!({
            "tool_name": event.tool_name,
            "error": event.error,
        }));
    }

    async fn user_prompt_submit(&self, event: &mut UserPromptSubmitEvent<'_>) {
        self.record("user_prompt_submit", event.conversation_id, serde_json::json!({
            "prompt": event.prompt,
        }));
    }

    async fn stop(&self, event: &StopEvent<'_>) -> StopDecision {
        let remaining = self.force_continue_count.load(Ordering::Relaxed);
        let will_continue = remaining > 0;
        if will_continue {
            self.force_continue_count.fetch_sub(1, Ordering::Relaxed);
        }
        self.record("stop", event.conversation_id, serde_json::json!({
            "stop_reason": format!("{:?}", event.stop_reason),
            "round_count": event.round_count,
            "decision": if will_continue { "Continue" } else { "Stop" },
        }));
        if will_continue {
            StopDecision::Continue("HookProbe forced continuation".to_string())
        } else {
            StopDecision::Stop
        }
    }

    async fn turn_end(&self, event: &TurnEndEvent<'_>) {
        self.record("turn_end", event.conversation_id, serde_json::json!({
            "run_id": event.run_id,
            "round_count": event.round_count,
            "turn_cost": event.turn_cost,
            "error": event.error,
        }));
    }

    async fn on_startup(&self) -> anyhow::Result<()> {
        self.record("on_startup", "", serde_json::json!({}));
        Ok(())
    }

    async fn on_shutdown(&self) -> anyhow::Result<()> {
        self.record("on_shutdown", "", serde_json::json!({}));
        Ok(())
    }

    async fn doctor(&self) -> DoctorReport {
        DoctorReport {
            module: "hook_probe".to_string(),
            status: DoctorStatus::Healthy,
            checks: vec![DoctorCheck {
                name: "active".to_string(),
                passed: true,
                message: "HookProbe active".to_string(),
            }],
        }
    }
}
