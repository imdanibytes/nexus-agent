use nexus_provider::types::Tool;

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
pub fn is_client_only(_tool_name: &str) -> bool {
    false // No client-only tools currently — all tools visible to model
}

/// Return Tool definitions for all built-in task tools.
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
