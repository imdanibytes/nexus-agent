use std::sync::Arc;

use async_trait::async_trait;

use crate::module::{
    DaemonModule, DoctorCheck, DoctorReport, DoctorStatus, PromptSection, TurnStartEvent,
};
use crate::tasks::TaskService;

/// Injects plan and task progress context into the status message via
/// the `turn_start` hook.
///
/// Replaces the old hardcoded `TaskContextProvider` in the system prompt
/// builder — modules now own their own context injection.
pub struct TaskContextModule {
    pub tasks: Arc<TaskService>,
}

#[async_trait]
impl DaemonModule for TaskContextModule {
    fn name(&self) -> &str {
        "task_context"
    }

    async fn turn_start(&self, event: &mut TurnStartEvent<'_>) {
        let task_state = match self.tasks.get(event.conversation_id).await {
            Some(ts) => ts,
            None => return,
        };

        let plan = match task_state.plan.as_ref() {
            Some(p) => p,
            None => return,
        };

        let mode = task_state.mode.to_string();
        let mut lines = Vec::new();

        lines.push(format!("Plan: \"{}\" ({} mode)", plan.title, mode));

        if let Some(ref summary) = plan.summary {
            lines.push(format!("Summary: {}", summary));
        }

        if !plan.task_ids.is_empty() {
            lines.push(String::new());
            lines.push("Tasks:".to_string());

            let tasks: Vec<_> = plan
                .task_ids
                .iter()
                .filter_map(|id| task_state.tasks.get(id))
                .collect();

            let completed = tasks.iter().filter(|t| t.status.to_string() == "completed").count();
            let total = tasks.len();

            let current_id = tasks
                .iter()
                .find(|t| t.status.to_string() == "in_progress")
                .or_else(|| tasks.iter().find(|t| t.status.to_string() == "pending"))
                .map(|t| t.id.clone());

            for (i, task) in tasks.iter().enumerate() {
                let is_current = current_id.as_deref() == Some(&task.id);
                let marker = if is_current { " ← CURRENT" } else { "" };
                let deps = if task.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(" (depends on: {})", task.depends_on.join(", "))
                };
                lines.push(format!(
                    "  [{}] {}. {}{}{}",
                    task.status,
                    i + 1,
                    task.title,
                    deps,
                    marker,
                ));
                if is_current {
                    if let Some(ref desc) = task.description {
                        lines.push(format!("    Description: {}", desc));
                    }
                }
            }

            lines.push(String::new());
            lines.push(format!("Progress: {}/{} completed", completed, total));

            if let Some(ref current_id) = current_id {
                if let Some(current) = tasks.iter().find(|t| t.id == *current_id) {
                    lines.push(format!("Current task: {}", current.title));
                    lines.push("Update this task's status with task_update as you work.".to_string());
                }
            }
        }

        event.status_sections.push(PromptSection {
            name: "task_context".to_string(),
            content: format!("<plan_context>\n{}\n</plan_context>", lines.join("\n")),
        });
    }

    async fn doctor(&self) -> DoctorReport {
        DoctorReport {
            module: "task_context".into(),
            status: DoctorStatus::Healthy,
            checks: vec![DoctorCheck {
                name: "task_service_available".into(),
                passed: true,
                message: "Task context module is active".into(),
            }],
        }
    }
}
