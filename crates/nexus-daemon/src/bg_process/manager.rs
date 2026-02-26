use std::collections::HashMap;
use std::path::PathBuf;

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
