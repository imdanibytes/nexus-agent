use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agent::events::{AgUiEvent, EventEnvelope};
use crate::server::message_queue::{MessageQueue, QueuedMessage};
use super::types::*;

const MAX_CONCURRENT_PER_CONVERSATION: usize = 5;
const PREVIEW_CHARS: usize = 500;

pub struct ProcessManager {
    processes: Mutex<HashMap<String, BgProcess>>,
    cancels: Mutex<HashMap<String, CancellationToken>>,
    base_dir: PathBuf,
    agent_tx: broadcast::Sender<EventEnvelope>,
    message_queue: Arc<MessageQueue>,
}

#[derive(Debug)]
pub struct SpawnResult {
    pub process_id: String,
    pub cancel_token: CancellationToken,
    pub output_path: PathBuf,
}

impl ProcessManager {
    pub fn new(
        base_dir: PathBuf,
        agent_tx: broadcast::Sender<EventEnvelope>,
        message_queue: Arc<MessageQueue>,
    ) -> Self {
        std::fs::create_dir_all(&base_dir).ok();
        Self {
            processes: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            base_dir,
            agent_tx,
            message_queue,
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
        let _ = self.agent_tx.send(EventEnvelope {
            thread_id: Some(conversation_id.to_string()),
            run_id: None,
            event: AgUiEvent::Custom {
                name: "bg_process_started".to_string(),
                value: serde_json::to_value(&process).unwrap_or_default(),
            },
        });

        Ok(SpawnResult {
            process_id: id,
            cancel_token: cancel,
            output_path,
        })
    }

    /// Mark a process as completed/failed and enqueue a notification message.
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

        // Emit SSE event for frontend
        let _ = self.agent_tx.send(EventEnvelope {
            thread_id: Some(conv_id.clone()),
            run_id: None,
            event: AgUiEvent::Custom {
                name: "bg_process_completed".to_string(),
                value: serde_json::to_value(&snapshot).unwrap_or_default(),
            },
        });

        // Enqueue notification as a user-role message
        let text = format_bg_notification(&snapshot);
        self.message_queue
            .enqueue(
                &conv_id,
                QueuedMessage {
                    text,
                    metadata: serde_json::json!({ "synthetic": true, "source": "bg_process" }),
                },
            )
            .await;
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

            let _ = self.agent_tx.send(EventEnvelope {
                thread_id: Some(conv_id),
                run_id: None,
                event: AgUiEvent::Custom {
                    name: "bg_process_cancelled".to_string(),
                    value: serde_json::to_value(&snapshot).unwrap_or_default(),
                },
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

        self.message_queue.clear(conversation_id).await;
    }

    /// Check if any conversation has active (running) processes.
    pub async fn has_running(&self, conversation_id: &str) -> bool {
        let procs = self.processes.lock().await;
        procs
            .values()
            .any(|p| p.conversation_id == conversation_id && p.status == ProcessStatus::Running)
    }
}

/// Format a completed background process into a notification message.
fn format_bg_notification(process: &BgProcess) -> String {
    let status = match process.status {
        ProcessStatus::Completed => "completed",
        ProcessStatus::Failed => "failed",
        ProcessStatus::Cancelled => "cancelled",
        ProcessStatus::Running => "running",
    };

    let mut text = format!(
        "[System] Background process \"{}\" (ID: {}) {}",
        process.label, process.id, status
    );

    if let Some(code) = process.exit_code {
        text.push_str(&format!(" | Exit code: {}", code));
    }

    text.push_str(&format!(" | Output: {} bytes", process.output_size));
    text.push_str(&format!(
        "\nUse process_output(process_id=\"{}\") to read the full output.",
        process.id
    ));

    if let Some(ref preview) = process.output_preview {
        text.push_str(&format!("\n\nPreview:\n{}", preview));
    }

    text
}

#[cfg(test)]
fn test_process_manager() -> ProcessManager {
    let dir = std::env::temp_dir().join(format!("nexus-bg-test-{}", Uuid::new_v4()));
    let (tx, _rx) = broadcast::channel(16);
    let (queue, _queue_rx) = MessageQueue::new();
    ProcessManager::new(dir, tx, Arc::new(queue))
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
    async fn complete_updates_status_and_enqueues_notification() {
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

        // Notification enqueued to message queue
        let queued = pm.message_queue.drain("conv1").await;
        assert_eq!(queued.len(), 1);
        assert!(queued[0].text.contains("test"));
        assert!(queued[0].text.contains("completed"));

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

        // Only one notification enqueued
        let queued = pm.message_queue.drain("conv1").await;
        assert_eq!(queued.len(), 1);

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
    fn format_notification_text() {
        let process = BgProcess {
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
        };

        let text = format_bg_notification(&process);
        assert!(text.contains("Running tests"));
        assert!(text.contains("abc123"));
        assert!(text.contains("completed"));
        assert!(text.contains("Exit code: 0"));
        assert!(text.contains("1234 bytes"));
        assert!(text.contains("all tests pass"));
        assert!(text.contains("process_output"));
    }
}
