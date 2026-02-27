use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ── Types ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    General,
    Discovery,
    Planning,
    Execution,
    Validation,
}

impl fmt::Display for AgentMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentMode::General => write!(f, "general"),
            AgentMode::Discovery => write!(f, "discovery"),
            AgentMode::Planning => write!(f, "planning"),
            AgentMode::Execution => write!(f, "execution"),
            AgentMode::Validation => write!(f, "validation"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_label: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    pub conversation_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub task_ids: Vec<String>,
    /// None = pending review, Some(true) = approved, Some(false) = rejected
    pub approved: Option<bool>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskState {
    pub plan: Option<Plan>,
    pub tasks: HashMap<String, Task>,
    pub mode: AgentMode,
}

impl Default for TaskState {
    fn default() -> Self {
        Self {
            plan: None,
            tasks: HashMap::new(),
            mode: AgentMode::General,
        }
    }
}

// ── Store ──

/// Per-conversation task state, persisted to disk as JSON.
pub struct TaskStateStore {
    states: HashMap<String, TaskState>,
    base_dir: PathBuf,
}

impl TaskStateStore {
    pub fn new(base_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&base_dir).ok();
        Self {
            states: HashMap::new(),
            base_dir,
        }
    }

    /// Get the task state for a conversation, with mode derived from state.
    /// Lazily loads from disk if not already in memory.
    pub fn get(&mut self, conversation_id: &str) -> Option<TaskState> {
        if !self.states.contains_key(conversation_id) {
            if let Some(state) = self.load_from_disk(conversation_id) {
                self.states.insert(conversation_id.to_string(), state);
            }
        }
        self.states.get(conversation_id).map(|s| {
            let mut state = s.clone();
            state.mode = derive_mode(&state);
            state
        })
    }

    /// Get or create the task state for a conversation.
    /// Lazily loads from disk if not already in memory.
    pub fn get_or_default(&mut self, conversation_id: &str) -> &mut TaskState {
        if !self.states.contains_key(conversation_id) {
            if let Some(state) = self.load_from_disk(conversation_id) {
                self.states.insert(conversation_id.to_string(), state);
            }
        }
        self.states
            .entry(conversation_id.to_string())
            .or_default()
    }

    /// Persist the task state for a conversation to disk.
    pub fn save(&self, conversation_id: &str) -> Result<(), String> {
        let state = match self.states.get(conversation_id) {
            Some(s) => s,
            None => return Ok(()),
        };
        let path = self.base_dir.join(format!("{}.json", conversation_id));
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| format!("Failed to serialize task state: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write task state to {}: {}", path.display(), e))
    }

    /// Remove task state for a conversation from memory and disk.
    pub fn remove(&mut self, conversation_id: &str) {
        self.states.remove(conversation_id);
        let path = self.base_dir.join(format!("{}.json", conversation_id));
        std::fs::remove_file(&path).ok();
    }

    /// Load task state from disk for a conversation.
    fn load_from_disk(&self, conversation_id: &str) -> Option<TaskState> {
        let path = self.base_dir.join(format!("{}.json", conversation_id));
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }
}

/// Derive the agent mode from the current task state.
///
/// - No plan → General
/// - Plan not yet approved (None) or rejected (false) → Planning
/// - Plan approved, tasks remaining → Execution
/// - Plan approved, all tasks completed → Validation
pub fn derive_mode(state: &TaskState) -> AgentMode {
    let plan = match &state.plan {
        Some(p) => p,
        None => return AgentMode::General,
    };

    match plan.approved {
        None | Some(false) => AgentMode::Planning,
        Some(true) => {
            if state.tasks.is_empty() {
                return AgentMode::Execution;
            }
            let all_done = state
                .tasks
                .values()
                .all(|t| matches!(t.status, TaskStatus::Completed | TaskStatus::Failed));
            if all_done {
                AgentMode::Validation
            } else {
                AgentMode::Execution
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_plan(approved: Option<bool>) -> Plan {
        Plan {
            id: "p1".into(),
            conversation_id: "c1".into(),
            title: "Test Plan".into(),
            summary: None,
            task_ids: vec![],
            approved,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn make_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.into(),
            title: format!("Task {}", id),
            description: None,
            status,
            parent_id: None,
            depends_on: vec![],
            active_label: None,
            created_at: 0,
            updated_at: 0,
            completed_at: None,
        }
    }

    #[test]
    fn no_plan_is_general() {
        let state = TaskState::default();
        assert_eq!(derive_mode(&state), AgentMode::General);
    }

    #[test]
    fn unapproved_plan_is_planning() {
        let state = TaskState {
            plan: Some(make_plan(None)),
            ..Default::default()
        };
        assert_eq!(derive_mode(&state), AgentMode::Planning);
    }

    #[test]
    fn rejected_plan_is_planning() {
        let state = TaskState {
            plan: Some(make_plan(Some(false))),
            ..Default::default()
        };
        assert_eq!(derive_mode(&state), AgentMode::Planning);
    }

    #[test]
    fn approved_with_pending_tasks_is_execution() {
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), make_task("t1", TaskStatus::Pending));
        tasks.insert("t2".into(), make_task("t2", TaskStatus::InProgress));
        let state = TaskState {
            plan: Some(make_plan(Some(true))),
            tasks,
            mode: AgentMode::General,
        };
        assert_eq!(derive_mode(&state), AgentMode::Execution);
    }

    #[test]
    fn approved_all_completed_is_validation() {
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), make_task("t1", TaskStatus::Completed));
        tasks.insert("t2".into(), make_task("t2", TaskStatus::Completed));
        let state = TaskState {
            plan: Some(make_plan(Some(true))),
            tasks,
            mode: AgentMode::General,
        };
        assert_eq!(derive_mode(&state), AgentMode::Validation);
    }

    #[test]
    fn approved_no_tasks_is_execution() {
        let state = TaskState {
            plan: Some(make_plan(Some(true))),
            tasks: HashMap::new(),
            mode: AgentMode::General,
        };
        assert_eq!(derive_mode(&state), AgentMode::Execution);
    }
}
