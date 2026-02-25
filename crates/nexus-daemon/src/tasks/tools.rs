use chrono::Utc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::agent::events::AgUiEvent;
use crate::anthropic::types::Tool;
use super::store::{TaskStateStore, derive_mode};
use super::types::{Plan, Task, TaskStatus};

/// Check if a tool name is a built-in task tool.
pub fn is_builtin(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "task_create_plan"
            | "task_approve_plan"
            | "task_create"
            | "task_update"
            | "task_list"
    )
}

/// Tools that are client-only: hidden from the model's tool list but
/// executable via the `POST /api/chat/tool-invoke` endpoint.
/// Inspired by MCP Apps `visibility: ["app"]` pattern.
pub fn is_client_only(_tool_name: &str) -> bool {
    false // No client-only tools currently — all tools visible to model
}

/// Return Anthropic Tool definitions for all built-in task tools.
pub fn definitions() -> Vec<Tool> {
    vec![
        Tool {
            name: "task_create_plan".into(),
            description:
                "Create a plan for the current conversation. Use this when the user's request \
                 involves multiple steps that benefit from structured planning."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "A concise title for the plan"
                    },
                    "summary": {
                        "type": "string",
                        "description": "A brief summary of the plan's goal and approach"
                    }
                },
                "required": ["title"]
            }),
        },
        Tool {
            name: "task_approve_plan".into(),
            description:
                "Mark the current plan as approved or rejected. IMPORTANT: Only call this AFTER \
                 using ask_user to get the user's confirmation. Never approve your own plans \
                 without user input."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "approved": {
                        "type": "boolean",
                        "description": "Whether the user approved the plan"
                    },
                    "feedback": {
                        "type": "string",
                        "description": "Optional feedback from the user"
                    }
                },
                "required": ["approved"]
            }),
        },
        Tool {
            name: "task_create".into(),
            description:
                "Add a task to the current plan. Tasks are executed in the order they are created."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "A concise, imperative title (e.g. 'Implement auth middleware')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Detailed description of what needs to be done"
                    },
                    "parent_id": {
                        "type": "string",
                        "description": "ID of the parent task (for subtask grouping)"
                    },
                    "depends_on": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "IDs of tasks that must complete before this one"
                    },
                    "active_label": {
                        "type": "string",
                        "description": "Present-continuous label shown while in progress (e.g. 'Implementing auth')"
                    }
                },
                "required": ["title"]
            }),
        },
        Tool {
            name: "task_update".into(),
            description:
                "Update a task's status or details. Use this to mark tasks as in_progress, \
                 completed, or failed as you work through the plan."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The task ID to update"
                    },
                    "status": {
                        "type": "string",
                        "enum": ["pending", "in_progress", "completed", "failed"],
                        "description": "New status for the task"
                    },
                    "title": {
                        "type": "string",
                        "description": "Updated title"
                    },
                    "description": {
                        "type": "string",
                        "description": "Updated description"
                    },
                    "active_label": {
                        "type": "string",
                        "description": "Updated active label"
                    }
                },
                "required": ["task_id"]
            }),
        },
        Tool {
            name: "task_list".into(),
            description:
                "List all tasks in the current plan with their status and dependencies.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
    ]
}

/// Handle a built-in task tool call. Returns (content_json, is_error).
pub async fn handle_builtin(
    tool_name: &str,
    args: &serde_json::Value,
    conversation_id: &str,
    task_store: &RwLock<TaskStateStore>,
    tx: &broadcast::Sender<AgUiEvent>,
) -> (String, bool) {
    let result = match tool_name {
        "task_create_plan" => handle_create_plan(args, conversation_id, task_store).await,
        "task_approve_plan" => handle_approve_plan(args, conversation_id, task_store).await,
        "task_create" => handle_create_task(args, conversation_id, task_store).await,
        "task_update" => handle_update_task(args, conversation_id, task_store).await,
        "task_list" => handle_list_tasks(conversation_id, task_store).await,
        _ => Err("Unknown built-in tool".into()),
    };

    match result {
        Ok(value) => {
            // Persist after mutations (skip read-only task_list)
            if tool_name != "task_list" {
                let store = task_store.read().await;
                if let Err(e) = store.save(conversation_id) {
                    tracing::warn!("Failed to persist task state: {}", e);
                }
            }

            // Emit task_state_changed event
            let mut store = task_store.write().await;
            if let Some(state) = store.get(conversation_id) {
                let _ = tx.send(AgUiEvent::Custom {
                    thread_id: conversation_id.to_string(),
                    name: "task_state_changed".to_string(),
                    value: serde_json::json!({
                        "conversationId": conversation_id,
                        "plan": state.plan,
                        "tasks": state.tasks,
                        "mode": state.mode,
                    }),
                });
            }
            (value.to_string(), false)
        }
        Err(msg) => (serde_json::json!({ "error": msg }).to_string(), true),
    }
}

async fn handle_create_plan(
    args: &serde_json::Value,
    conversation_id: &str,
    task_store: &RwLock<TaskStateStore>,
) -> Result<serde_json::Value, String> {
    let title = args["title"]
        .as_str()
        .ok_or("Missing required field: title")?;
    let summary = args["summary"].as_str().map(|s| s.to_string());

    let now = Utc::now().timestamp_millis();
    let plan = Plan {
        id: Uuid::new_v4().to_string(),
        conversation_id: conversation_id.to_string(),
        title: title.to_string(),
        summary,
        task_ids: vec![],
        approved: None,
        created_at: now,
        updated_at: now,
    };

    let mut store = task_store.write().await;
    let state = store.get_or_default(conversation_id);

    if state.plan.is_some() {
        return Err("A plan already exists for this conversation. Update or clear it first.".into());
    }

    state.plan = Some(plan.clone());
    state.mode = derive_mode(state);

    Ok(serde_json::json!({
        "plan": plan,
        "mode": state.mode,
    }))
}

async fn handle_approve_plan(
    args: &serde_json::Value,
    conversation_id: &str,
    task_store: &RwLock<TaskStateStore>,
) -> Result<serde_json::Value, String> {
    let approved = args["approved"]
        .as_bool()
        .ok_or("Missing required field: approved")?;

    let mut store = task_store.write().await;
    let state = store.get_or_default(conversation_id);

    let plan = state
        .plan
        .as_mut()
        .ok_or("No plan exists to approve")?;

    plan.approved = Some(approved);
    plan.updated_at = Utc::now().timestamp_millis();
    state.mode = derive_mode(state);

    let feedback = args["feedback"].as_str().map(|s| s.to_string());

    Ok(serde_json::json!({
        "plan": state.plan,
        "approved": approved,
        "feedback": feedback,
        "mode": state.mode,
    }))
}

async fn handle_create_task(
    args: &serde_json::Value,
    conversation_id: &str,
    task_store: &RwLock<TaskStateStore>,
) -> Result<serde_json::Value, String> {
    let title = args["title"]
        .as_str()
        .ok_or("Missing required field: title")?;

    let mut store = task_store.write().await;
    let state = store.get_or_default(conversation_id);

    if state.plan.is_none() {
        return Err("No plan exists. Create a plan first with task_create_plan.".into());
    }

    let parent_id = args["parent_id"].as_str().map(|s| s.to_string());
    if let Some(ref pid) = parent_id {
        if !state.tasks.contains_key(pid) {
            return Err(format!("Parent task '{}' does not exist", pid));
        }
    }

    let depends_on: Vec<String> = args["depends_on"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    for dep in &depends_on {
        if !state.tasks.contains_key(dep) {
            return Err(format!("Dependency task '{}' does not exist", dep));
        }
    }

    let now = Utc::now().timestamp_millis();
    let task_id = Uuid::new_v4().to_string();

    let task = Task {
        id: task_id.clone(),
        title: title.to_string(),
        description: args["description"].as_str().map(|s| s.to_string()),
        status: TaskStatus::Pending,
        parent_id,
        depends_on,
        active_label: args["active_label"].as_str().map(|s| s.to_string()),
        created_at: now,
        updated_at: now,
        completed_at: None,
    };

    state.tasks.insert(task_id.clone(), task.clone());
    if let Some(ref mut plan) = state.plan {
        plan.task_ids.push(task_id);
        plan.updated_at = now;
    }
    state.mode = derive_mode(state);

    Ok(serde_json::json!({
        "task": task,
        "mode": state.mode,
    }))
}

async fn handle_update_task(
    args: &serde_json::Value,
    conversation_id: &str,
    task_store: &RwLock<TaskStateStore>,
) -> Result<serde_json::Value, String> {
    let task_id = args["task_id"]
        .as_str()
        .ok_or("Missing required field: task_id")?;

    let mut store = task_store.write().await;
    let state = store.get_or_default(conversation_id);

    let task = state
        .tasks
        .get_mut(task_id)
        .ok_or_else(|| format!("Task '{}' not found", task_id))?;

    let now = Utc::now().timestamp_millis();

    if let Some(status_str) = args["status"].as_str() {
        let status: TaskStatus = serde_json::from_value(serde_json::json!(status_str))
            .map_err(|_| format!("Invalid status: '{}'", status_str))?;
        task.status = status;
        if matches!(status, TaskStatus::Completed) {
            task.completed_at = Some(now);
        }
    }
    if let Some(title) = args["title"].as_str() {
        task.title = title.to_string();
    }
    if let Some(desc) = args["description"].as_str() {
        task.description = Some(desc.to_string());
    }
    if let Some(label) = args["active_label"].as_str() {
        task.active_label = Some(label.to_string());
    }

    task.updated_at = now;

    let task_clone = task.clone();
    state.mode = derive_mode(state);
    let mode = state.mode;

    Ok(serde_json::json!({
        "task": task_clone,
        "mode": mode,
    }))
}

async fn handle_list_tasks(
    conversation_id: &str,
    task_store: &RwLock<TaskStateStore>,
) -> Result<serde_json::Value, String> {
    let mut store = task_store.write().await;
    let state = match store.get(conversation_id) {
        Some(s) => s,
        None => {
            return Ok(serde_json::json!({
                "plan": null,
                "tasks": [],
                "mode": "general",
            }));
        }
    };

    // Return tasks in plan order
    let ordered_tasks: Vec<&Task> = match &state.plan {
        Some(plan) => plan
            .task_ids
            .iter()
            .filter_map(|id| state.tasks.get(id))
            .collect(),
        None => state.tasks.values().collect(),
    };

    Ok(serde_json::json!({
        "plan": state.plan,
        "tasks": ordered_tasks,
        "mode": state.mode,
    }))
}
