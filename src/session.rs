use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AgentError;

/// Persists agent state so it can stop and resume at the exact same spot.
#[async_trait]
pub trait SessionManager: Send + Sync {
    /// Save a checkpoint of the current agent state.
    async fn checkpoint(&self, session_id: &str, state: &SessionState) -> Result<(), AgentError>;

    /// Load the most recent checkpoint for a session.
    async fn load(&self, session_id: &str) -> Result<Option<SessionState>, AgentError>;
}

/// Everything needed to resume an agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub turn: usize,
    pub context_snapshot: Value,
    pub pending_tool_calls: Vec<PendingToolCall>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

// --- NoSessionManager ---

/// No persistence. Fire-and-forget.
pub struct NoSessionManager;

#[async_trait]
impl SessionManager for NoSessionManager {
    async fn checkpoint(&self, _: &str, _: &SessionState) -> Result<(), AgentError> {
        Ok(())
    }

    async fn load(&self, _: &str) -> Result<Option<SessionState>, AgentError> {
        Ok(None)
    }
}

// --- FileSessionManager ---

/// Saves session state to disk as JSON.
pub struct FileSessionManager {
    dir: PathBuf,
}

impl FileSessionManager {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

#[async_trait]
impl SessionManager for FileSessionManager {
    async fn checkpoint(&self, session_id: &str, state: &SessionState) -> Result<(), AgentError> {
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| AgentError::Session(e.to_string()))?;
        let path = self.dir.join(format!("{session_id}.json"));
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| AgentError::Session(e.to_string()))?;
        tokio::fs::write(path, json)
            .await
            .map_err(|e| AgentError::Session(e.to_string()))?;
        Ok(())
    }

    async fn load(&self, session_id: &str) -> Result<Option<SessionState>, AgentError> {
        let path = self.dir.join(format!("{session_id}.json"));
        match tokio::fs::read_to_string(path).await {
            Ok(json) => {
                let state: SessionState = serde_json::from_str(&json)
                    .map_err(|e| AgentError::Session(e.to_string()))?;
                Ok(Some(state))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AgentError::Session(e.to_string())),
        }
    }
}
