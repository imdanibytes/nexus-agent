use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agent::events::AgUiEvent;
use super::types::*;

const MAX_CONCURRENT_PER_CONVERSATION: usize = 5;
const PREVIEW_CHARS: usize = 500;

pub struct ProcessManager {
    processes: Mutex<HashMap<String, BgProcess>>,
    cancels: Mutex<HashMap<String, CancellationToken>>,
    pending_notifications: Mutex<HashMap<String, Vec<PendingNotification>>>,
    base_dir: PathBuf,
    agent_tx: broadcast::Sender<AgUiEvent>,
}

#[derive(Debug)]
pub struct SpawnResult {
    pub process_id: String,
    pub cancel_token: CancellationToken,
    pub output_path: PathBuf,
}

impl ProcessManager {
    pub fn new(base_dir: PathBuf, agent_tx: broadcast::Sender<AgUiEvent>) -> Self {
        std::fs::create_dir_all(&base_dir).ok();
        Self {
            processes: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            pending_notifications: Mutex::new(HashMap::new()),
            base_dir,
            agent_tx,
        }
    }

    /// Register a new background process. Returns spawn info or error if at capacity.
    pub async fn spawn(
        &self,
        conversation_id: &str,
        label: String,
        command: String,
        kind: ProcessKind,
    ) -> Result<SpawnResult, String> {
        let mut procs = self.processes.lock().await;

        let active_count = procs
            .values()
            .filter(|p| p.conversation_id == conversation_id && p.status == ProcessStatus::Running)
            .count();

        if active_count >= MAX_CONCURRENT_PER_CONVERSATION {
            return Err(format!(
                "Maximum {} concurrent background processes per conversation. \
                 Wait for running processes to finish or stop one first.",
                MAX_CONCURRENT_PER_CONVERSATION
            ));
        }

        let id = Uuid::new_v4().to_string();
        let output_path = self.base_dir.join(format!("{}.out", id));

        let process = BgProcess {
            id: id.clone(),
            conversation_id: conversation_id.to_string(),
            label: label.clone(),
            command,
            kind,
            status: ProcessStatus::Running,
            started_at: Utc::now(),
            completed_at: None,
            exit_code: None,
            is_error: false,
            output_path: output_path.clone(),
            output_preview: None,
            output_size: 0,
        };

        procs.insert(id.clone(), process.clone());
        drop(procs);

        let cancel = CancellationToken::new();
        self.cancels.lock().await.insert(id.clone(), cancel.clone());

        // Emit SSE event for frontend
        let _ = self.agent_tx.send(AgUiEvent::Custom {
            thread_id: conversation_id.to_string(),
            name: "bg_process_started".to_string(),
            value: serde_json::to_value(&process).unwrap_or_default(),
        });

        Ok(SpawnResult {
            process_id: id,
            cancel_token: cancel,
            output_path,
        })
    }

    /// Mark a process as completed/failed and queue a notification.
    pub async fn complete(
        &self,
        process_id: &str,
        exit_code: Option<i32>,
        is_error: bool,
    ) {
        let mut procs = self.processes.lock().await;
        let Some(proc) = procs.get_mut(process_id) else {
            return;
        };
        if proc.status != ProcessStatus::Running {
            return;
        }

        proc.status = if is_error {
            ProcessStatus::Failed
        } else {
            ProcessStatus::Completed
        };
        proc.completed_at = Some(Utc::now());
        proc.exit_code = exit_code;
        proc.is_error = is_error;

        // Read output preview + size
        if let Ok(meta) = std::fs::metadata(&proc.output_path) {
            proc.output_size = meta.len();
        }
        if let Ok(content) = std::fs::read_to_string(&proc.output_path) {
            if !content.is_empty() {
                let preview: String = content.chars().take(PREVIEW_CHARS).collect();
                proc.output_preview = Some(preview);
            }
        }

        let snapshot = proc.clone();
        let conv_id = proc.conversation_id.clone();
        drop(procs);

        // Remove cancel token
        self.cancels.lock().await.remove(process_id);

        // Queue notification
        let mut pending = self.pending_notifications.lock().await;
        pending
            .entry(conv_id.clone())
            .or_default()
            .push(PendingNotification {
                process: snapshot.clone(),
            });

        // Emit SSE event for frontend
        let _ = self.agent_tx.send(AgUiEvent::Custom {
            thread_id: conv_id,
            name: "bg_process_completed".to_string(),
            value: serde_json::to_value(&snapshot).unwrap_or_default(),
        });
    }

    /// Cancel a running process.
    pub async fn cancel(&self, process_id: &str) -> Result<(), String> {
        let cancels = self.cancels.lock().await;
        let Some(token) = cancels.get(process_id) else {
            return Err("Process not found or already finished".to_string());
        };
        token.cancel();
        drop(cancels);

        let mut procs = self.processes.lock().await;
        if let Some(proc) = procs.get_mut(process_id) {
            proc.status = ProcessStatus::Cancelled;
            proc.completed_at = Some(Utc::now());

            let snapshot = proc.clone();
            let conv_id = proc.conversation_id.clone();
            drop(procs);

            self.cancels.lock().await.remove(process_id);

            let _ = self.agent_tx.send(AgUiEvent::Custom {
                thread_id: conv_id,
                name: "bg_process_cancelled".to_string(),
                value: serde_json::to_value(&snapshot).unwrap_or_default(),
            });

            Ok(())
        } else {
            Err("Process not found".to_string())
        }
    }

    /// List all processes for a conversation.
    pub async fn list(&self, conversation_id: &str) -> Vec<BgProcess> {
        let procs = self.processes.lock().await;
        procs
            .values()
            .filter(|p| p.conversation_id == conversation_id)
            .cloned()
            .collect()
    }

    /// Read output from a process file. Supports tail (last N lines) and head (first N lines).
    pub async fn read_output(
        &self,
        process_id: &str,
        tail: Option<usize>,
        head: Option<usize>,
    ) -> Result<String, String> {
        let procs = self.processes.lock().await;
        let Some(proc) = procs.get(process_id) else {
            return Err("Process not found".to_string());
        };
        let path = proc.output_path.clone();
        drop(procs);

        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read output: {e}"))?;

        if let Some(n) = tail {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(n);
            Ok(lines[start..].join("\n"))
        } else if let Some(n) = head {
            let lines: Vec<&str> = content.lines().take(n).collect();
            Ok(lines.join("\n"))
        } else {
            // Limit to 100k chars
            if content.len() > 100_000 {
                Ok(format!(
                    "{}\n... (output truncated, {} total bytes. Use tail/head to read specific sections.)",
                    &content[..100_000],
                    content.len()
                ))
            } else {
                Ok(content)
            }
        }
    }

    /// Drain all pending notifications for a conversation.
    pub async fn drain_notifications(&self, conversation_id: &str) -> Vec<PendingNotification> {
        let mut pending = self.pending_notifications.lock().await;
        pending.remove(conversation_id).unwrap_or_default()
    }

    /// Check if there are pending notifications for a conversation.
    pub async fn has_pending(&self, conversation_id: &str) -> bool {
        let pending = self.pending_notifications.lock().await;
        pending
            .get(conversation_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Get all conversation IDs that have pending notifications.
    pub async fn conversations_with_pending(&self) -> Vec<String> {
        let pending = self.pending_notifications.lock().await;
        pending
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Cancel all running processes for a conversation and clean up output files.
    pub async fn cleanup_conversation(&self, conversation_id: &str) {
        let mut procs = self.processes.lock().await;
        let ids: Vec<String> = procs
            .values()
            .filter(|p| p.conversation_id == conversation_id)
            .map(|p| p.id.clone())
            .collect();

        let mut cancels = self.cancels.lock().await;
        for id in &ids {
            if let Some(token) = cancels.remove(id) {
                token.cancel();
            }
            if let Some(proc) = procs.remove(id) {
                let _ = std::fs::remove_file(&proc.output_path);
            }
        }
        drop(cancels);
        drop(procs);

        self.pending_notifications
            .lock()
            .await
            .remove(conversation_id);
    }

    /// Check if any conversation has active (running) processes.
    pub async fn has_running(&self, conversation_id: &str) -> bool {
        let procs = self.processes.lock().await;
        procs
            .values()
            .any(|p| p.conversation_id == conversation_id && p.status == ProcessStatus::Running)
    }
}

/// Start a background task that polls for pending notifications on idle conversations.
///
/// When a background process completes while no agent turn is active for that conversation,
/// this watcher picks up the notification and spawns a follow-up turn.
pub fn start_idle_watcher(
    process_manager: Arc<ProcessManager>,
    state: Arc<crate::server::AppState>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            interval.tick().await;

            let pending_convs = process_manager.conversations_with_pending().await;
            if pending_convs.is_empty() {
                continue;
            }

            for conv_id in pending_convs {
                // Skip if a turn is currently active for this conversation
                let active = state.chat.active_cancel.lock().await;
                let turn_active = active
                    .as_ref()
                    .map(|(id, _)| id == &conv_id)
                    .unwrap_or(false);
                drop(active);

                if turn_active {
                    continue;
                }

                let notifications = process_manager.drain_notifications(&conv_id).await;
                if notifications.is_empty() {
                    continue;
                }

                tracing::info!(
                    conversation_id = %conv_id,
                    count = notifications.len(),
                    "Idle watcher: injecting background process notifications"
                );

                // Load conversation
                let conv = {
                    let store = state.chat.conversations.read().await;
                    match store.get(&conv_id) {
                        Ok(Some(c)) => c,
                        Ok(None) => {
                            tracing::warn!(conversation_id = %conv_id, "Idle watcher: conversation not found");
                            continue;
                        }
                        Err(e) => {
                            tracing::warn!(conversation_id = %conv_id, "Idle watcher: failed to load conversation: {}", e);
                            continue;
                        }
                    }
                };

                crate::server::turn::inject_notifications_and_follow_up(
                    Arc::clone(&state),
                    conv,
                    conv_id,
                    notifications,
                )
                .await;
            }
        }
    })
}

#[cfg(test)]
fn test_process_manager() -> ProcessManager {
    let dir = std::env::temp_dir().join(format!("nexus-bg-test-{}", Uuid::new_v4()));
    let (tx, _rx) = broadcast::channel(16);
    ProcessManager::new(dir, tx)
}

/// Format pending notifications into a synthetic user message.
pub fn format_notifications(notifications: &[PendingNotification]) -> String {
    let mut text = String::from("[System] Background process completed.\n");

    for notif in notifications {
        let p = &notif.process;
        let status = match p.status {
            ProcessStatus::Completed => "completed",
            ProcessStatus::Failed => "failed",
            ProcessStatus::Cancelled => "cancelled",
            ProcessStatus::Running => "running",
        };

        text.push_str(&format!(
            "\nProcess: \"{}\" (ID: {})\nStatus: {}",
            p.label, p.id, status
        ));

        if let Some(code) = p.exit_code {
            text.push_str(&format!(" | Exit code: {}", code));
        }

        text.push_str(&format!(" | Output: {} bytes", p.output_size));

        text.push_str(&format!(
            "\nUse process_output(process_id=\"{}\") to read the full output.",
            p.id
        ));

        if let Some(ref preview) = p.output_preview {
            text.push_str(&format!("\n\nPreview:\n{}", preview));
        }

        text.push('\n');
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pm() -> ProcessManager {
        test_process_manager()
    }

    #[tokio::test]
    async fn spawn_returns_process_id_and_output_path() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "echo hi".into(), ProcessKind::Bash)
            .await
            .unwrap();

        assert!(!result.process_id.is_empty());
        assert!(result.output_path.to_str().unwrap().contains(&result.process_id));

        let listed = pm.list("conv1").await;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, ProcessStatus::Running);
        assert_eq!(listed[0].label, "test");

        // Cleanup
        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn spawn_enforces_max_concurrent() {
        let pm = make_pm();
        for i in 0..MAX_CONCURRENT_PER_CONVERSATION {
            pm.spawn("conv1", format!("p{i}"), format!("cmd{i}"), ProcessKind::Bash)
                .await
                .unwrap();
        }

        let err = pm
            .spawn("conv1", "overflow".into(), "cmd".into(), ProcessKind::Bash)
            .await
            .unwrap_err();
        assert!(err.contains("Maximum"));

        // Different conversation is not affected
        let ok = pm
            .spawn("conv2", "other".into(), "cmd".into(), ProcessKind::Bash)
            .await;
        assert!(ok.is_ok());

        pm.cleanup_conversation("conv1").await;
        pm.cleanup_conversation("conv2").await;
    }

    #[tokio::test]
    async fn complete_updates_status_and_queues_notification() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "echo hi".into(), ProcessKind::Bash)
            .await
            .unwrap();

        // Write some output
        std::fs::write(&result.output_path, "hello world").unwrap();

        pm.complete(&result.process_id, Some(0), false).await;

        let listed = pm.list("conv1").await;
        assert_eq!(listed[0].status, ProcessStatus::Completed);
        assert_eq!(listed[0].exit_code, Some(0));
        assert!(!listed[0].is_error);
        assert_eq!(listed[0].output_preview.as_deref(), Some("hello world"));
        assert_eq!(listed[0].output_size, 11);

        // Notification queued
        assert!(pm.has_pending("conv1").await);
        let notifs = pm.drain_notifications("conv1").await;
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].process.id, result.process_id);

        // Drained — no more pending
        assert!(!pm.has_pending("conv1").await);

        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn complete_with_error_sets_failed_status() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "fail".into(), ProcessKind::Bash)
            .await
            .unwrap();

        pm.complete(&result.process_id, Some(1), true).await;

        let listed = pm.list("conv1").await;
        assert_eq!(listed[0].status, ProcessStatus::Failed);
        assert!(listed[0].is_error);
        assert_eq!(listed[0].exit_code, Some(1));

        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn complete_ignores_non_running_process() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "cmd".into(), ProcessKind::Bash)
            .await
            .unwrap();

        pm.complete(&result.process_id, Some(0), false).await;
        // Second complete should be a no-op
        pm.complete(&result.process_id, Some(1), true).await;

        let listed = pm.list("conv1").await;
        assert_eq!(listed[0].status, ProcessStatus::Completed);
        assert_eq!(listed[0].exit_code, Some(0)); // unchanged

        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn cancel_sets_cancelled_status() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "sleep 99".into(), ProcessKind::Bash)
            .await
            .unwrap();

        assert!(result.cancel_token.is_cancelled() == false);
        pm.cancel(&result.process_id).await.unwrap();
        assert!(result.cancel_token.is_cancelled());

        let listed = pm.list("conv1").await;
        assert_eq!(listed[0].status, ProcessStatus::Cancelled);
        assert!(listed[0].completed_at.is_some());

        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn cancel_returns_error_for_finished_process() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "cmd".into(), ProcessKind::Bash)
            .await
            .unwrap();
        pm.complete(&result.process_id, Some(0), false).await;

        let err = pm.cancel(&result.process_id).await.unwrap_err();
        assert!(err.contains("not found or already finished"));

        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn read_output_full() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "cmd".into(), ProcessKind::Bash)
            .await
            .unwrap();
        std::fs::write(&result.output_path, "line1\nline2\nline3\n").unwrap();

        let output = pm.read_output(&result.process_id, None, None).await.unwrap();
        assert_eq!(output, "line1\nline2\nline3\n");

        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn read_output_tail() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "cmd".into(), ProcessKind::Bash)
            .await
            .unwrap();
        std::fs::write(&result.output_path, "line1\nline2\nline3").unwrap();

        let output = pm.read_output(&result.process_id, Some(1), None).await.unwrap();
        assert_eq!(output, "line3");

        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn read_output_head() {
        let pm = make_pm();
        let result = pm
            .spawn("conv1", "test".into(), "cmd".into(), ProcessKind::Bash)
            .await
            .unwrap();
        std::fs::write(&result.output_path, "line1\nline2\nline3").unwrap();

        let output = pm.read_output(&result.process_id, None, Some(2)).await.unwrap();
        assert_eq!(output, "line1\nline2");

        pm.cleanup_conversation("conv1").await;
    }

    #[tokio::test]
    async fn read_output_not_found() {
        let pm = make_pm();
        let err = pm.read_output("nonexistent", None, None).await.unwrap_err();
        assert!(err.contains("not found"));
    }

    #[tokio::test]
    async fn drain_returns_empty_for_no_notifications() {
        let pm = make_pm();
        let notifs = pm.drain_notifications("conv1").await;
        assert!(notifs.is_empty());
    }

    #[tokio::test]
    async fn conversations_with_pending() {
        let pm = make_pm();
        let r1 = pm
            .spawn("conv1", "a".into(), "a".into(), ProcessKind::Bash)
            .await
            .unwrap();
        let r2 = pm
            .spawn("conv2", "b".into(), "b".into(), ProcessKind::Bash)
            .await
            .unwrap();

        pm.complete(&r1.process_id, Some(0), false).await;
        pm.complete(&r2.process_id, Some(0), false).await;

        let mut pending = pm.conversations_with_pending().await;
        pending.sort();
        assert_eq!(pending, vec!["conv1", "conv2"]);

        pm.drain_notifications("conv1").await;
        let pending = pm.conversations_with_pending().await;
        assert_eq!(pending, vec!["conv2"]);

        pm.cleanup_conversation("conv1").await;
        pm.cleanup_conversation("conv2").await;
    }

    #[tokio::test]
    async fn cleanup_cancels_running_and_removes_files() {
        let pm = make_pm();
        let r1 = pm
            .spawn("conv1", "a".into(), "a".into(), ProcessKind::Bash)
            .await
            .unwrap();

        std::fs::write(&r1.output_path, "data").unwrap();
        assert!(r1.output_path.exists());

        pm.cleanup_conversation("conv1").await;

        assert!(r1.cancel_token.is_cancelled());
        assert!(!r1.output_path.exists());
        assert!(pm.list("conv1").await.is_empty());
        assert!(!pm.has_pending("conv1").await);
    }

    #[tokio::test]
    async fn has_running() {
        let pm = make_pm();
        assert!(!pm.has_running("conv1").await);

        let result = pm
            .spawn("conv1", "test".into(), "cmd".into(), ProcessKind::Bash)
            .await
            .unwrap();
        assert!(pm.has_running("conv1").await);

        pm.complete(&result.process_id, Some(0), false).await;
        assert!(!pm.has_running("conv1").await);

        pm.cleanup_conversation("conv1").await;
    }

    #[test]
    fn format_notifications_single() {
        let notif = PendingNotification {
            process: BgProcess {
                id: "abc123".to_string(),
                conversation_id: "conv1".to_string(),
                label: "Running tests".to_string(),
                command: "cargo test".to_string(),
                kind: ProcessKind::Bash,
                status: ProcessStatus::Completed,
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                exit_code: Some(0),
                is_error: false,
                output_path: PathBuf::from("/tmp/test.out"),
                output_preview: Some("all tests pass".to_string()),
                output_size: 1234,
            },
        };

        let text = format_notifications(&[notif]);
        assert!(text.contains("Background process completed"));
        assert!(text.contains("Running tests"));
        assert!(text.contains("abc123"));
        assert!(text.contains("completed"));
        assert!(text.contains("Exit code: 0"));
        assert!(text.contains("1234 bytes"));
        assert!(text.contains("all tests pass"));
        assert!(text.contains("process_output"));
    }

    #[test]
    fn format_notifications_multiple() {
        let notifs: Vec<PendingNotification> = (0..3)
            .map(|i| PendingNotification {
                process: BgProcess {
                    id: format!("p{i}"),
                    conversation_id: "conv1".to_string(),
                    label: format!("Task {i}"),
                    command: format!("cmd{i}"),
                    kind: ProcessKind::Bash,
                    status: if i == 1 {
                        ProcessStatus::Failed
                    } else {
                        ProcessStatus::Completed
                    },
                    started_at: Utc::now(),
                    completed_at: Some(Utc::now()),
                    exit_code: Some(if i == 1 { 1 } else { 0 }),
                    is_error: i == 1,
                    output_path: PathBuf::from("/tmp/test.out"),
                    output_preview: None,
                    output_size: 0,
                },
            })
            .collect();

        let text = format_notifications(&notifs);
        assert!(text.contains("Task 0"));
        assert!(text.contains("Task 1"));
        assert!(text.contains("Task 2"));
        assert!(text.contains("failed"));
    }

    #[test]
    fn format_notifications_empty() {
        let text = format_notifications(&[]);
        assert!(text.contains("Background process completed"));
    }
}
