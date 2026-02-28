pub use nexus_tools::tasks::{definitions, is_builtin, is_client_only};

use tokio::sync::RwLock;

use crate::agent::emitter::TurnEmitter;
use nexus_core::tasks::TaskStateStore;

/// Handle a built-in task tool call. Returns (content_json, is_error).
///
/// Orchestrates locking, persistence, and event emission around the pure
/// execution logic in `nexus_tools::tasks::execute`.
pub async fn handle_builtin(
    tool_name: &str,
    args: &serde_json::Value,
    conversation_id: &str,
    task_store: &RwLock<TaskStateStore>,
    emitter: &TurnEmitter,
) -> (String, bool) {
    let is_read_only = tool_name == "task_list";

    let result = {
        let mut store = task_store.write().await;
        let state = if is_read_only {
            match store.get(conversation_id) {
                Some(s) => s,
                None => {
                    return (
                        serde_json::json!({
                            "plan": null,
                            "tasks": [],
                            "mode": "general",
                        })
                        .to_string(),
                        false,
                    );
                }
            }
        } else {
            store.get_or_default(conversation_id).clone()
        };

        let mut state = state;
        // Set conversation_id on new plans (crate doesn't know it)
        if tool_name == "task_create_plan" {
            // Will be set after execute creates the plan
        }

        let result = nexus_tools::tasks::execute(tool_name, args, &mut state);

        // Patch conversation_id on newly created plans
        if let Some(ref mut plan) = state.plan {
            if plan.conversation_id.is_empty() {
                plan.conversation_id = conversation_id.to_string();
            }
        }

        // Write state back to store
        if !is_read_only {
            *store.get_or_default(conversation_id) = state;
        }

        result
    };

    let (content, is_error) = result;

    if !is_error && !is_read_only {
        // Persist
        let store = task_store.read().await;
        if let Err(e) = store.save(conversation_id) {
            tracing::warn!("Failed to persist task state: {}", e);
        }
    }

    if !is_error {
        // Emit state changed event
        let event_payload = {
            let mut store = task_store.write().await;
            store.get(conversation_id).map(|state| {
                serde_json::json!({
                    "conversationId": conversation_id,
                    "plan": state.plan,
                    "tasks": state.tasks,
                    "mode": state.mode,
                })
            })
        };
        if let Some(payload) = event_payload {
            emitter.custom("task_state_changed", payload);
        }
    }

    (content, is_error)
}
