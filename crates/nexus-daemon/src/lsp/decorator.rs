use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::tool_dispatch::{ToolContext, ToolHandler, ToolResult};

use super::diagnostics::DiagnosticStatus;
use super::manager::DiagnosticReport;
use super::LspService;

/// Tools whose results should be decorated with cached diagnostics.
const READ_TOOLS: &[&str] = &["read_text_file", "read_file"];

/// Tools that trigger didChange + wait for fresh diagnostics.
const WRITE_TOOLS: &[&str] = &["write_file", "edit_file"];

/// Wraps FilesystemHandler, decorating results with LSP diagnostics
/// for files within the conversation's workspace projects.
pub struct LspDecoratedFsHandler {
    pub inner: crate::agent::tool_dispatch::FilesystemHandler,
    pub lsp: Arc<LspService>,
    /// Project paths from the conversation's active workspace.
    pub project_paths: Vec<String>,
}

#[async_trait]
impl ToolHandler for LspDecoratedFsHandler {
    fn can_handle(&self, tool_name: &str) -> bool {
        self.inner.can_handle(tool_name)
    }

    async fn handle(&self, ctx: &ToolContext<'_>) -> ToolResult {
        // Delegate to inner filesystem handler
        let result = self.inner.handle(ctx).await;
        if result.is_error {
            return result;
        }

        // Check if LSP is globally enabled
        {
            let configs = self.lsp.configs.read().await;
            if !configs.settings().enabled {
                return result;
            }
        }

        // Extract file path from args
        let file_path = match extract_file_path(ctx.args_json) {
            Some(p) => p,
            None => return result,
        };

        // Check if file is within a workspace project
        let project_path = match self.find_project_for_file(&file_path) {
            Some(p) => p,
            None => {
                tracing::debug!(
                    file = %file_path,
                    project_count = self.project_paths.len(),
                    "File not within any project path, skipping LSP"
                );
                return result;
            }
        };

        // Get diagnostics based on operation type
        let is_write = WRITE_TOOLS.contains(&ctx.tool_name);
        let is_read = READ_TOOLS.contains(&ctx.tool_name);

        let report = if is_write {
            let content = read_file_content(&file_path).await;
            let mut manager = self.lsp.manager.write().await;
            manager.diagnostics_after_write(
                &file_path,
                &content,
                &project_path,
            ).await
        } else if is_read {
            let mut manager = self.lsp.manager.write().await;
            manager.diagnostics_after_read(&file_path, &project_path).await
        } else {
            return result;
        };

        let report = match report {
            Some(r) => r,
            None => return result,
        };

        let decoration = format_report(&report);
        tracing::debug!(
            tool = %ctx.tool_name,
            file = %file_path,
            server = %report.server_name,
            "LSP decoration: {}",
            match &report.status {
                DiagnosticStatus::Ready(d) => format!("{} diagnostic(s)", d.len()),
                DiagnosticStatus::Pending => "pending (indexing)".to_string(),
            }
        );

        if decoration.is_empty() {
            return result;
        }

        ToolResult {
            content: format!("{}\n\n{}", result.content, decoration),
            is_error: false,
        }
    }
}

impl LspDecoratedFsHandler {
    /// Find which project path contains this file.
    fn find_project_for_file(&self, file_path: &str) -> Option<String> {
        self.project_paths
            .iter()
            .find(|p| file_path.starts_with(p.as_str()))
            .cloned()
    }
}

/// Extract the primary file path from tool arguments.
fn extract_file_path(args_json: &str) -> Option<String> {
    let args: serde_json::Value = serde_json::from_str(args_json).ok()?;
    // Most file tools use "path" as the field name
    args.get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Read file content from disk (for write/edit operations where we need
/// the final content to send to LSP).
async fn read_file_content(path: &str) -> String {
    tokio::fs::read_to_string(path).await.unwrap_or_default()
}

/// Format a diagnostic report for inclusion in tool results.
fn format_report(report: &DiagnosticReport) -> String {
    match &report.status {
        DiagnosticStatus::Ready(diagnostics) => format_diagnostics(diagnostics),
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
    fn extract_file_path_from_args() {
        let args = r#"{"path": "/home/user/src/main.rs", "description": "read file"}"#;
        assert_eq!(extract_file_path(args), Some("/home/user/src/main.rs".into()));
    }

    #[test]
    fn extract_file_path_missing_returns_none() {
        assert_eq!(extract_file_path(r#"{"content": "hello"}"#), None);
    }

    #[test]
    fn extract_file_path_invalid_json_returns_none() {
        assert_eq!(extract_file_path("not json"), None);
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
        // Info-level diagnostics are excluded
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
        assert!(output.is_empty(), "Clean file should produce no decoration");
    }
}
