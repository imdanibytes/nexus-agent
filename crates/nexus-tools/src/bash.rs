use nexus_provider::types::Tool;

const TOOL_NAME: &str = "bash";
const DEFAULT_TIMEOUT_MS: u64 = 120_000; // 2 minutes
const MAX_TIMEOUT_MS: u64 = 600_000; // 10 minutes
const MAX_OUTPUT_CHARS: usize = 100_000;

pub fn is_bash(name: &str) -> bool {
    name == TOOL_NAME
}

pub fn tool_definition() -> Tool {
    Tool {
        name: TOOL_NAME.to_string(),
        description: "Execute a bash command and return its stdout/stderr. \
            Commands run in a shell with a configurable timeout (default 2 minutes, max 10 minutes). \
            The working directory is the first allowed directory. \
            Use this for: running builds, tests, git operations, package management, \
            and other command-line tasks. \
            Do NOT use this for reading files (use read_file), writing files (use write_file), \
            or searching files (use search_files). \
            Avoid interactive commands that require stdin input."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute.",
                },
                "timeout_ms": {
                    "type": "integer",
                    "default": DEFAULT_TIMEOUT_MS,
                    "minimum": 1000,
                    "maximum": MAX_TIMEOUT_MS,
                    "description": "Timeout in milliseconds (default 120000, max 600000).",
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "Run the command in the background. Returns immediately with a process ID. \
                        Use process_output to read output, process_status to check status, \
                        and process_stop to cancel. You will be notified when the process completes.",
                },
            },
            "required": ["command"],
        }),
    }
}

/// Execute a bash command with timeout and output limits.
///
/// Returns `(output_text, is_error)`.
pub async fn execute(command: &str, timeout_ms: Option<u64>, working_dir: Option<&str>) -> (String, bool) {
    let timeout = timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .min(MAX_TIMEOUT_MS);

    let shell = detect_shell();

    let mut cmd = tokio::process::Command::new(&shell);
    cmd.arg("-c").arg(command);

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    // Don't inherit stdin — prevent interactive commands from hanging
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(timeout),
        cmd.output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            let mut text = String::new();

            if !stdout.is_empty() {
                text.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str("STDERR:\n");
                text.push_str(&stderr);
            }

            if text.is_empty() {
                text = format!("(no output, exit code {})", exit_code);
            }

            // Truncate if too long
            if text.len() > MAX_OUTPUT_CHARS {
                text.truncate(MAX_OUTPUT_CHARS);
                text.push_str("\n... (output truncated)");
            }

            let is_error = !output.status.success();
            if is_error && !text.contains("exit code") {
                text.push_str(&format!("\n(exit code {})", exit_code));
            }

            (text, is_error)
        }
        Ok(Err(e)) => {
            (format!("Failed to execute command: {}", e), true)
        }
        Err(_) => {
            (format!("Command timed out after {}ms", timeout), true)
        }
    }
}

/// Detect the user's preferred shell.
fn detect_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_identity() {
        assert!(is_bash("bash"));
        assert!(!is_bash("shell"));
        assert!(!is_bash(""));
    }

    #[tokio::test]
    async fn execute_simple_command() {
        let (output, is_error) = execute("echo hello", None, None).await;
        assert!(!is_error);
        assert!(output.contains("hello"));
    }

    #[tokio::test]
    async fn execute_failing_command() {
        let (output, is_error) = execute("false", None, None).await;
        assert!(is_error);
        assert!(output.contains("exit code"));
    }

    #[tokio::test]
    async fn execute_with_stderr() {
        let (output, is_error) = execute("echo err >&2", None, None).await;
        assert!(!is_error);
        assert!(output.contains("STDERR:"));
        assert!(output.contains("err"));
    }

    #[tokio::test]
    async fn execute_timeout() {
        let (output, is_error) = execute("sleep 60", Some(100), None).await;
        assert!(is_error);
        assert!(output.contains("timed out"));
    }

    #[tokio::test]
    async fn execute_with_working_dir() {
        let (output, is_error) = execute("pwd", None, Some("/tmp")).await;
        assert!(!is_error);
        // /tmp may resolve to /private/tmp on macOS
        assert!(output.contains("tmp"));
    }
}
