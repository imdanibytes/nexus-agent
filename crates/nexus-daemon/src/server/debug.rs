//! Debug endpoints — only compiled in debug builds.
//!
//! These endpoints let you trigger internal events and state transitions
//! from the browser or curl without needing real API calls.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::agent::events::{AgUiEvent, EventEnvelope};
use crate::conversation::types::Span;
use crate::server::AppState;
use crate::tasks::types::{AgentMode, Plan, Task, TaskState, TaskStatus};

/// Force-compact a conversation by moving old messages out of active_path
/// and inserting a synthetic summary. No LLM call needed.
///
/// POST /api/debug/compact/:id
/// Body: { "keep_recent": 4 } (optional, defaults to 4)
pub async fn force_compact(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<ForceCompactRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let keep_recent = body.map(|b| b.keep_recent).unwrap_or(4);

    let mut conv = state.threads.checkout(&id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if conv.active_path.len() <= keep_recent {
        return Ok(Json(serde_json::json!({
            "compacted": false,
            "reason": "Not enough messages to compact",
        })));
    }

    let split = conv.active_path.len() - keep_recent;
    let consumed_ids: Vec<String> = conv.active_path[..split].to_vec();

    // Build synthetic summary
    let summary_text = format!(
        "[Debug compaction — {} messages summarized]\n\n\
         This is a synthetic summary created by the debug endpoint.\n\
         Original messages are preserved in the conversation history.",
        consumed_ids.len(),
    );

    // Create spans: seal current, open new
    if conv.spans.is_empty() {
        conv.spans.push(Span {
            index: 0,
            message_ids: consumed_ids.clone(),
            summary: Some(summary_text),
            sealed_at: Some(Utc::now()),
        });
        conv.spans.push(Span {
            index: 1,
            message_ids: Vec::new(),
            summary: None,
            sealed_at: None,
        });
    } else {
        conv.seal_current_span(&consumed_ids, summary_text);
        conv.open_new_span();
    }

    // Remove consumed from active_path
    conv.active_path.retain(|id| !consumed_ids.contains(id));
    conv.updated_at = Utc::now();

    let sealed_index = conv.spans.len() - 2;

    state.threads.commit(conv).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Emit the compaction event
    let _ = state.turns.event_bridge.agent_tx().send(EventEnvelope {
        thread_id: Some(id.clone()),
        run_id: None,
        event: AgUiEvent::Custom {
            name: "compaction".to_string(),
            value: serde_json::json!({
                "sealed_span_index": sealed_index,
                "consumed_count": consumed_ids.len(),
            }),
        },
    });

    Ok(Json(serde_json::json!({
        "compacted": true,
        "consumed_count": consumed_ids.len(),
        "sealed_span_index": sealed_index,
    })))
}

/// Set the task state for a conversation to a preset.
///
/// POST /api/debug/task-state/:id
/// Body: { "preset": "planning" | "execution" | "execution_progress" | "validation" | "clear" }
pub async fn set_task_state(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SetTaskStateRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let now = Utc::now().timestamp_millis();

    let task_state = match body.preset.as_str() {
        "planning" => {
            let plan = Plan {
                id: Uuid::new_v4().to_string(),
                conversation_id: id.clone(),
                title: "Debug: Sample Plan".to_string(),
                summary: Some("This is a debug plan for testing the planning UI.".to_string()),
                task_ids: vec![
                    "t1".to_string(),
                    "t2".to_string(),
                    "t3".to_string(),
                ],
                approved: None,
                created_at: now,
                updated_at: now,
            };
            let mut tasks = std::collections::HashMap::new();
            for (i, tid) in ["t1", "t2", "t3"].iter().enumerate() {
                tasks.insert(
                    tid.to_string(),
                    Task {
                        id: tid.to_string(),
                        title: format!("Task {}: Do something", i + 1),
                        description: Some(format!("Description for task {}", i + 1)),
                        status: TaskStatus::Pending,
                        parent_id: None,
                        depends_on: if i > 0 {
                            vec![format!("t{}", i)]
                        } else {
                            vec![]
                        },
                        active_label: None,
                        created_at: now,
                        updated_at: now,
                        completed_at: None,
                    },
                );
            }
            TaskState {
                plan: Some(plan),
                tasks,
                mode: AgentMode::Planning,
            }
        }
        "execution" => {
            let plan = Plan {
                id: Uuid::new_v4().to_string(),
                conversation_id: id.clone(),
                title: "Debug: Approved Plan".to_string(),
                summary: Some("Testing execution mode with an approved plan.".to_string()),
                task_ids: vec![
                    "t1".to_string(),
                    "t2".to_string(),
                    "t3".to_string(),
                    "t4".to_string(),
                    "t5".to_string(),
                ],
                approved: Some(true),
                created_at: now,
                updated_at: now,
            };
            let titles = [
                "Set up project structure",
                "Implement core logic",
                "Add error handling",
                "Write tests",
                "Update documentation",
            ];
            let mut tasks = std::collections::HashMap::new();
            for (i, title) in titles.iter().enumerate() {
                let tid = format!("t{}", i + 1);
                tasks.insert(
                    tid.clone(),
                    Task {
                        id: tid,
                        title: title.to_string(),
                        description: Some(format!("Detailed description for: {}", title)),
                        status: TaskStatus::Pending,
                        parent_id: None,
                        depends_on: if i > 0 {
                            vec![format!("t{}", i)]
                        } else {
                            vec![]
                        },
                        active_label: None,
                        created_at: now,
                        updated_at: now,
                        completed_at: None,
                    },
                );
            }
            TaskState {
                plan: Some(plan),
                tasks,
                mode: AgentMode::Execution,
            }
        }
        "execution_progress" => {
            let plan = Plan {
                id: Uuid::new_v4().to_string(),
                conversation_id: id.clone(),
                title: "Debug: In-Progress Plan".to_string(),
                summary: Some("Plan with mixed task statuses for UI testing.".to_string()),
                task_ids: vec![
                    "t1".to_string(),
                    "t2".to_string(),
                    "t3".to_string(),
                    "t4".to_string(),
                    "t5".to_string(),
                ],
                approved: Some(true),
                created_at: now,
                updated_at: now,
            };
            let specs: Vec<(&str, TaskStatus)> = vec![
                ("Set up project structure", TaskStatus::Completed),
                ("Implement core logic", TaskStatus::Completed),
                ("Add error handling", TaskStatus::InProgress),
                ("Write tests", TaskStatus::Pending),
                ("Update documentation", TaskStatus::Pending),
            ];
            let mut tasks = std::collections::HashMap::new();
            for (i, (title, status)) in specs.iter().enumerate() {
                let tid = format!("t{}", i + 1);
                tasks.insert(
                    tid.clone(),
                    Task {
                        id: tid,
                        title: title.to_string(),
                        description: Some(format!("Detailed description for: {}", title)),
                        status: *status,
                        parent_id: None,
                        depends_on: if i > 0 {
                            vec![format!("t{}", i)]
                        } else {
                            vec![]
                        },
                        active_label: if *status == TaskStatus::InProgress {
                            Some("Adding try/catch blocks...".to_string())
                        } else {
                            None
                        },
                        created_at: now,
                        updated_at: now,
                        completed_at: if *status == TaskStatus::Completed {
                            Some(now)
                        } else {
                            None
                        },
                    },
                );
            }
            TaskState {
                plan: Some(plan),
                tasks,
                mode: AgentMode::Execution,
            }
        }
        "validation" => {
            let plan = Plan {
                id: Uuid::new_v4().to_string(),
                conversation_id: id.clone(),
                title: "Debug: Completed Plan".to_string(),
                summary: Some("All tasks done, ready for validation.".to_string()),
                task_ids: vec![
                    "t1".to_string(),
                    "t2".to_string(),
                    "t3".to_string(),
                ],
                approved: Some(true),
                created_at: now,
                updated_at: now,
            };
            let mut tasks = std::collections::HashMap::new();
            for i in 0..3 {
                let tid = format!("t{}", i + 1);
                tasks.insert(
                    tid.clone(),
                    Task {
                        id: tid,
                        title: format!("Completed task {}", i + 1),
                        description: None,
                        status: TaskStatus::Completed,
                        parent_id: None,
                        depends_on: vec![],
                        active_label: None,
                        created_at: now,
                        updated_at: now,
                        completed_at: Some(now),
                    },
                );
            }
            TaskState {
                plan: Some(plan),
                tasks,
                mode: AgentMode::Validation,
            }
        }
        "clear" => TaskState::default(),
        _ => {
            return Ok(Json(serde_json::json!({
                "error": "Unknown preset. Use: planning, execution, execution_progress, validation, clear",
            })));
        }
    };

    let mode = task_state.mode;

    // Persist task state (TaskService::set handles saving + event emission)
    if let Err(e) = state.tasks.set(&id, task_state).await {
        tracing::warn!("Failed to persist debug task state: {}", e);
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "mode": mode.to_string(),
    })))
}

/// Emit a custom AG-UI event.
///
/// POST /api/debug/emit
/// Body: { "thread_id": "...", "name": "...", "value": { ... } }
pub async fn emit_event(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EmitEventRequest>,
) -> Json<serde_json::Value> {
    let _ = state.turns.event_bridge.agent_tx().send(EventEnvelope {
        thread_id: Some(body.thread_id),
        run_id: None,
        event: AgUiEvent::Custom {
            name: body.name,
            value: body.value,
        },
    });
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
pub struct ForceCompactRequest {
    #[serde(default = "default_keep_recent")]
    pub keep_recent: usize,
}

fn default_keep_recent() -> usize {
    4
}

#[derive(Debug, Deserialize)]
pub struct SetTaskStateRequest {
    pub preset: String,
}

#[derive(Debug, Deserialize)]
pub struct EmitEventRequest {
    pub thread_id: String,
    pub name: String,
    #[serde(default)]
    pub value: serde_json::Value,
}
