use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    Bash,
    SubAgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BgProcess {
    pub id: String,
    pub conversation_id: String,
    pub label: String,
    pub command: String,
    pub kind: ProcessKind,
    pub status: ProcessStatus,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub is_error: bool,
    #[serde(skip)]
    pub output_path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_preview: Option<String>,
    pub output_size: u64,
}

#[derive(Debug, Clone)]
pub struct PendingNotification {
    pub process: BgProcess,
}
