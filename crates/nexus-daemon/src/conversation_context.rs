use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::config::FilesystemConfig;
use crate::module::{
    DaemonModule, DoctorCheck, DoctorReport, DoctorStatus, PromptSection, TurnStartEvent,
};
use crate::project::ProjectStore;
use crate::thread::ThreadService;
use crate::workspace::WorkspaceStore;

/// Injects workspace, project, working directory, and cost context into
/// the status message via the `turn_start` hook.
///
/// Replaces the old hardcoded `ConversationContextProvider` in the system
/// prompt builder — modules now own their own context injection.
pub struct ConversationContextModule {
    pub workspaces: Arc<RwLock<WorkspaceStore>>,
    pub projects: Arc<RwLock<ProjectStore>>,
    pub threads: Arc<ThreadService>,
    pub effective_fs: Arc<RwLock<FilesystemConfig>>,
}

#[async_trait]
impl DaemonModule for ConversationContextModule {
    fn name(&self) -> &str {
        "conversation_context"
    }

    async fn turn_start(&self, event: &mut TurnStartEvent<'_>) {
        let mut lines = Vec::new();

        // Workspace context
        if let Some((ws_name, ws_desc, ws_projects)) =
            self.resolve_workspace(event.conversation_id).await
        {
            lines.push(format!("Workspace: \"{}\"", ws_name));
            if let Some(desc) = ws_desc {
                lines.push(format!("Description: {}", desc));
            }
            if !ws_projects.is_empty() {
                lines.push(String::new());
                lines.push("Projects:".to_string());
                for (proj_name, proj_path) in &ws_projects {
                    lines.push(format!("- {} ({})", proj_name, proj_path));
                }
                lines.push(String::new());
            }
        }

        // Working directory
        let fs = self.effective_fs.read().await;
        if let Some(dir) = fs.allowed_directories.first() {
            lines.push(format!("Working directory: {}", dir));
        }
        drop(fs);

        // Conversation cost
        if let Ok(Some(conv)) = self.threads.get(event.conversation_id).await {
            let cost = conv.usage.as_ref().map(|u| u.total_cost).unwrap_or(0.0);
            if cost > 0.0 {
                lines.push(format!("Conversation cost: ${:.3}", cost));
            }
        }

        if !lines.is_empty() {
            event.status_sections.push(PromptSection {
                name: "conversation_context".to_string(),
                content: format!(
                    "<conversation_context>\n{}\n</conversation_context>",
                    lines.join("\n"),
                ),
            });
        }
    }

    async fn doctor(&self) -> DoctorReport {
        DoctorReport {
            module: "conversation_context".into(),
            status: DoctorStatus::Healthy,
            checks: vec![DoctorCheck {
                name: "context_available".into(),
                passed: true,
                message: "Conversation context module is active".into(),
            }],
        }
    }
}

impl ConversationContextModule {
    async fn resolve_workspace(
        &self,
        conversation_id: &str,
    ) -> Option<(String, Option<String>, Vec<(String, String)>)> {
        let workspace_id = self
            .threads
            .get(conversation_id)
            .await
            .ok()
            .flatten()?
            .workspace_id?;

        let ws_store = self.workspaces.read().await;
        let ws = ws_store.get(&workspace_id)?.clone();
        drop(ws_store);

        let proj_store = self.projects.read().await;
        let projects: Vec<(String, String)> = ws
            .project_ids
            .iter()
            .filter_map(|pid| {
                proj_store
                    .get(pid)
                    .map(|p| (p.name.clone(), p.path.clone()))
            })
            .collect();

        Some((ws.name, ws.description, projects))
    }
}
