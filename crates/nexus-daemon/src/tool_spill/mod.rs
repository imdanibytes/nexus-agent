//! Tool result spilling — saves oversized tool output to temp files.
//!
//! When a tool result exceeds ~30k chars (~10k tokens), the full output is
//! written to `/tmp/nexus-tool-output/` and the content is replaced with a
//! compact stub pointing to the file. Prevents blowing the context window.

use async_trait::async_trait;

use crate::module::{
    DaemonModule, DoctorCheck, DoctorReport, DoctorStatus, PostToolUseEvent,
};

const MAX_CHARS: usize = 30_000;
const OUTPUT_DIR: &str = "/tmp/nexus-tool-output";

pub struct ToolSpillModule;

#[async_trait]
impl DaemonModule for ToolSpillModule {
    fn name(&self) -> &str {
        "tool_spill"
    }

    async fn post_tool_use(&self, event: &mut PostToolUseEvent<'_>) {
        if event.result.content.len() > MAX_CHARS {
            event.result.content = spill_to_file(
                event.tool_name,
                event.tool_call_id,
                &event.result.content,
            );
        }
    }

    async fn doctor(&self) -> DoctorReport {
        let dir = std::path::Path::new(OUTPUT_DIR);
        let writable = std::fs::create_dir_all(dir).is_ok();
        DoctorReport {
            module: "tool_spill".into(),
            status: if writable {
                DoctorStatus::Healthy
            } else {
                DoctorStatus::Degraded
            },
            checks: vec![DoctorCheck {
                name: "output_dir_writable".into(),
                passed: writable,
                message: if writable {
                    format!("{} is writable", OUTPUT_DIR)
                } else {
                    format!("{} is not writable — large outputs will be truncated", OUTPUT_DIR)
                },
            }],
        }
    }
}

/// Save a large tool result to a temp file and return a compact stub.
///
/// The stub tells the model the file path, size, and a truncated preview,
/// so it can read the full output via bash if needed.
fn spill_to_file(tool_name: &str, tool_call_id: &str, content: &str) -> String {
    let dir = std::path::PathBuf::from(OUTPUT_DIR);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("Failed to create tool output dir: {}", e);
        return truncate_fallback(content);
    }

    let id_prefix = &tool_call_id[..8.min(tool_call_id.len())];
    let filename = format!("{}_{}.txt", tool_name, id_prefix);
    let path = dir.join(&filename);

    match std::fs::write(&path, content) {
        Ok(_) => {
            let preview: String = content.chars().take(500).collect();
            let suffix = if content.chars().count() > 500 { "…" } else { "" };
            tracing::info!(
                tool = tool_name,
                chars = content.len(),
                path = %path.display(),
                "Tool result spilled to file"
            );
            format!(
                "[Output saved to file: {} chars (~{} tokens)]\n\
                 Path: {}\n\
                 Use `bash cat {}` to read the full output if needed.\n\n\
                 Preview:\n{}{}",
                content.len(),
                content.len() / 3,
                path.display(),
                path.display(),
                preview,
                suffix,
            )
        }
        Err(e) => {
            tracing::warn!("Failed to write tool output file: {}", e);
            truncate_fallback(content)
        }
    }
}

fn truncate_fallback(content: &str) -> String {
    let preview: String = content.chars().take(2000).collect();
    format!(
        "[Output too large: {} chars (~{} tokens), truncated]\n\n{}…",
        content.len(),
        content.len() / 3,
        preview,
    )
}
