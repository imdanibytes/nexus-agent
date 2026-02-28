use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::module::{
    DaemonModule, DoctorCheck, DoctorReport, DoctorStatus, InjectedMessage, PostToolUseEvent,
};
use crate::project::ProjectStore;
use crate::thread::ThreadService;
use crate::workspace::WorkspaceStore;

use nexus_lsp::diagnostics::DiagnosticStatus;
use nexus_lsp::manager::DiagnosticReport;
use nexus_lsp::LspService;

/// Tools whose results should be decorated with cached diagnostics.
const READ_TOOLS: &[&str] = &["read_text_file", "read_file"];

/// Tools that trigger didChange + wait for fresh diagnostics.
const WRITE_TOOLS: &[&str] = &["write_file", "edit_file"];

/// LSP integration as a DaemonModule.
///
/// Replaces the old `LspDecoratedFsHandler` wrapper — instead of wrapping
/// the filesystem tool handler, this module hooks into `post_tool_use` to
/// decorate file tool results with LSP diagnostics.
///
/// Also handles LSP server warm-up (`on_startup`) and shutdown (`on_shutdown`).
pub struct LspModule {
    pub lsp: Arc<LspService>,
    pub projects: Arc<RwLock<ProjectStore>>,
    pub workspaces: Arc<RwLock<WorkspaceStore>>,
    pub threads: Arc<ThreadService>,
}

#[async_trait]
impl DaemonModule for LspModule {
    fn name(&self) -> &str {
        "lsp"
    }

    async fn on_startup(&self) -> anyhow::Result<()> {
        let project_paths: Vec<String> = {
            let ps = self.projects.read().await;
            ps.list().iter().map(|p| p.path.clone()).collect()
        };
        if project_paths.is_empty() {
            return Ok(());
        }
        tracing::info!(
            project_count = project_paths.len(),
            "LspModule: warming up LSP servers"
        );
        self.lsp.manager.write().await.warm_up(&project_paths).await;
        Ok(())
    }

    async fn on_shutdown(&self) -> anyhow::Result<()> {
        tracing::info!("LspModule: shutting down LSP servers");
        if tokio::time::timeout(std::time::Duration::from_secs(3), async {
            self.lsp.manager.read().await.shutdown_all().await;
        })
        .await
        .is_err()
        {
            tracing::warn!("LspModule: LSP shutdown timed out");
        }
        Ok(())
    }

    async fn post_tool_use(&self, event: &mut PostToolUseEvent<'_>) {
        let tool_name = event.tool_name;
        let is_write = WRITE_TOOLS.contains(&tool_name);
        let is_read = READ_TOOLS.contains(&tool_name);
        if !is_write && !is_read {
            return;
        }

        // Check if LSP is globally enabled
        {
            let configs = self.lsp.configs.read().await;
            if !configs.settings().enabled {
                return;
            }
        }

        // Extract file path from tool input
        let file_path = match event.tool_input.get("path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return,
        };

        // Resolve project paths for this conversation
        let project_paths = self.resolve_project_paths(event.conversation_id).await;
        if project_paths.is_empty() {
            return;
        }

        // Check if file is within a project
        let project_path = match find_project_for_file(&project_paths, &file_path) {
            Some(p) => p,
            None => {
                tracing::debug!(
                    file = %file_path,
                    project_count = project_paths.len(),
                    "File not within any project path, skipping LSP"
                );
                return;
            }
        };

        // Get diagnostics based on operation type
        let report = if is_write {
            let content = read_file_content(&file_path).await;
            let mut manager = self.lsp.manager.write().await;
            manager
                .diagnostics_after_write(&file_path, &content, &project_path)
                .await
        } else {
            let mut manager = self.lsp.manager.write().await;
            manager
                .diagnostics_after_read(&file_path, &project_path)
                .await
        };

        let report = match report {
            Some(r) => r,
            None => {
                tracing::info!(
                    tool = %tool_name,
                    file = %file_path,
                    "LSP: no server available for this file type"
                );
                return;
            }
        };

        let decoration = format_report(&report);
        tracing::info!(
            tool = %tool_name,
            file = %file_path,
            server = %report.server_name,
            "LSP decoration: {}",
            match &report.status {
                DiagnosticStatus::Ready(d) if d.is_empty() => "clean".to_string(),
                DiagnosticStatus::Ready(d) => format!("{} diagnostic(s)", d.len()),
                DiagnosticStatus::Pending => "pending (indexing)".to_string(),
            }
        );

        event
            .result
            .injected_messages
            .push(InjectedMessage { text: decoration });
    }

    async fn doctor(&self) -> DoctorReport {
        let configs = self.lsp.configs.read().await;
        let enabled = configs.settings().enabled;
        let server_count = configs.servers().len();
        let enabled_count = configs.servers().iter().filter(|c| c.enabled).count();

        DoctorReport {
            module: "lsp".to_string(),
            status: if enabled {
                DoctorStatus::Healthy
            } else {
                DoctorStatus::Disabled
            },
            checks: vec![
                DoctorCheck {
                    name: "LSP enabled".to_string(),
                    passed: enabled,
                    message: if enabled {
                        format!("{}/{} servers enabled", enabled_count, server_count)
                    } else {
                        "LSP globally disabled".to_string()
                    },
                },
            ],
        }
    }
}

impl LspModule {
    /// Resolve project paths for a conversation.
    /// Uses the conversation's workspace if set, otherwise falls back to all projects.
    async fn resolve_project_paths(&self, conversation_id: &str) -> Vec<String> {
        // Get the conversation's workspace_id
        let workspace_id = match self.threads.get(conversation_id).await {
            Ok(Some(conv)) => conv.workspace_id,
            _ => None,
        };

        if let Some(ws_id) = workspace_id {
            let ws_store = self.workspaces.read().await;
            if let Some(ws) = ws_store.get(&ws_id) {
                let project_ids = ws.project_ids.clone();
                drop(ws_store);
                let proj_store = self.projects.read().await;
                let paths: Vec<String> = project_ids
                    .iter()
                    .filter_map(|pid| proj_store.get(pid).map(|p| p.path.clone()))
                    .collect();
                if !paths.is_empty() {
                    return paths;
                }
            }
        }

        // Fallback: all configured projects
        let proj_store = self.projects.read().await;
        proj_store.list().iter().map(|p| p.path.clone()).collect()
    }
}

/// Find which project path contains this file.
fn find_project_for_file(project_paths: &[String], file_path: &str) -> Option<String> {
    project_paths
        .iter()
        .find(|p| file_path.starts_with(p.as_str()))
        .cloned()
}

/// Read file content from disk (for write/edit operations where we need
/// the final content to send to LSP).
async fn read_file_content(path: &str) -> String {
    tokio::fs::read_to_string(path).await.unwrap_or_default()
}

/// Format a diagnostic report for inclusion in tool results.
fn format_report(report: &DiagnosticReport) -> String {
    match &report.status {
        DiagnosticStatus::Ready(diagnostics) => {
            let body = format_diagnostics(diagnostics);
            if body.is_empty() {
                format!(
                    "<lsp_diagnostics server=\"{}\" status=\"clean\" />",
                    report.server_name,
                )
            } else {
                body
            }
        }
        DiagnosticStatus::Pending => {
            format!(
                "<lsp_diagnostics server=\"{}\" status=\"indexing\">\n\
                 Server is still indexing the project. Diagnostics will be available on subsequent file operations.\n\
                 </lsp_diagnostics>",
                report.server_name,
            )
        }
    }
}

/// Format diagnostics for inclusion in tool results.
fn format_diagnostics(diagnostics: &[lsp_types::Diagnostic]) -> String {
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR))
        .collect();
    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Some(lsp_types::DiagnosticSeverity::WARNING))
        .collect();

    if errors.is_empty() && warnings.is_empty() {
        return String::new();
    }

    let mut output = String::from("<diagnostics>\n");

    if !errors.is_empty() {
        output.push_str(&format!("{} error(s):\n", errors.len()));
        for d in &errors {
            output.push_str(&format!(
                "  L{}: {}\n",
                d.range.start.line + 1,
                d.message
            ));
        }
    }
    if !warnings.is_empty() {
        output.push_str(&format!("{} warning(s):\n", warnings.len()));
        for d in &warnings {
            output.push_str(&format!(
                "  L{}: {}\n",
                d.range.start.line + 1,
                d.message
            ));
        }
    }

    output.push_str("</diagnostics>");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

    fn make_diagnostic(line: u32, msg: &str, severity: DiagnosticSeverity) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position { line, character: 0 },
                end: Position { line, character: 0 },
            },
            severity: Some(severity),
            message: msg.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn find_project_for_file_matches() {
        let paths = vec!["/home/user/project".to_string()];
        assert_eq!(
            find_project_for_file(&paths, "/home/user/project/src/main.rs"),
            Some("/home/user/project".to_string())
        );
    }

    #[test]
    fn find_project_for_file_no_match() {
        let paths = vec!["/home/user/project".to_string()];
        assert_eq!(
            find_project_for_file(&paths, "/other/path/main.rs"),
            None
        );
    }

    #[test]
    fn format_diagnostics_empty() {
        assert_eq!(format_diagnostics(&[]), "");
    }

    #[test]
    fn format_diagnostics_errors_only() {
        let diags = vec![
            make_diagnostic(14, "expected `bool`, found `String`", DiagnosticSeverity::ERROR),
            make_diagnostic(41, "cannot find value `foo`", DiagnosticSeverity::ERROR),
        ];
        let output = format_diagnostics(&diags);
        assert!(output.contains("<diagnostics>"));
        assert!(output.contains("2 error(s):"));
        assert!(output.contains("L15: expected `bool`, found `String`"));
        assert!(output.contains("L42: cannot find value `foo`"));
        assert!(!output.contains("warning"));
    }

    #[test]
    fn format_diagnostics_warnings_only() {
        let diags = vec![
            make_diagnostic(7, "unused variable `x`", DiagnosticSeverity::WARNING),
        ];
        let output = format_diagnostics(&diags);
        assert!(output.contains("1 warning(s):"));
        assert!(output.contains("L8: unused variable `x`"));
        assert!(!output.contains("error"));
    }

    #[test]
    fn format_diagnostics_mixed() {
        let diags = vec![
            make_diagnostic(0, "error msg", DiagnosticSeverity::ERROR),
            make_diagnostic(5, "warning msg", DiagnosticSeverity::WARNING),
            make_diagnostic(9, "info msg", DiagnosticSeverity::INFORMATION),
        ];
        let output = format_diagnostics(&diags);
        assert!(output.contains("1 error(s):"));
        assert!(output.contains("1 warning(s):"));
        assert!(!output.contains("info msg"));
    }

    #[test]
    fn format_diagnostics_skips_hints() {
        let diags = vec![
            make_diagnostic(0, "hint msg", DiagnosticSeverity::HINT),
        ];
        assert_eq!(format_diagnostics(&diags), "");
    }

    #[test]
    fn format_report_pending() {
        let report = DiagnosticReport {
            status: DiagnosticStatus::Pending,
            server_name: "rust-analyzer".into(),
        };
        let output = format_report(&report);
        assert!(output.contains("rust-analyzer"));
        assert!(output.contains("indexing"));
    }

    #[test]
    fn format_report_ready_with_errors() {
        let report = DiagnosticReport {
            status: DiagnosticStatus::Ready(vec![
                make_diagnostic(0, "bad code", DiagnosticSeverity::ERROR),
            ]),
            server_name: "rust-analyzer".into(),
        };
        let output = format_report(&report);
        assert!(output.contains("<diagnostics>"));
        assert!(output.contains("bad code"));
    }

    #[test]
    fn format_report_ready_clean_file() {
        let report = DiagnosticReport {
            status: DiagnosticStatus::Ready(vec![]),
            server_name: "rust-analyzer".into(),
        };
        let output = format_report(&report);
        assert!(output.contains("status=\"clean\""), "Clean file should show clean status");
        assert!(output.contains("rust-analyzer"));
    }
}
