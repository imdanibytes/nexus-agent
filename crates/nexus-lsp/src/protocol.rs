use std::sync::atomic::{AtomicI64, Ordering};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

/// Content-Length framed JSON-RPC message writer for LSP.
pub struct LspWriter {
    stdin: ChildStdin,
    next_id: AtomicI64,
}

impl LspWriter {
    pub fn new(stdin: ChildStdin) -> Self {
        Self {
            stdin,
            next_id: AtomicI64::new(1),
        }
    }

    /// Send a JSON-RPC request. Returns the request ID.
    pub async fn request(&mut self, method: &str, params: serde_json::Value) -> Result<i64> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.send_message(&msg).await?;
        Ok(id)
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn notify(&mut self, method: &str, params: serde_json::Value) -> Result<()> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.send_message(&msg).await
    }

    async fn send_message(&mut self, msg: &serde_json::Value) -> Result<()> {
        let body = serde_json::to_string(msg)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }
}

/// Parsed LSP JSON-RPC message.
#[allow(dead_code)]
pub enum LspMessage {
    Response { id: i64, result: serde_json::Value },
    Error { id: i64, error: serde_json::Value },
    Notification { method: String, params: serde_json::Value },
}

/// Content-Length framed JSON-RPC message reader for LSP.
pub struct LspReader {
    reader: BufReader<ChildStdout>,
}

impl LspReader {
    pub fn new(stdout: ChildStdout) -> Self {
        Self {
            reader: BufReader::new(stdout),
        }
    }

    /// Read the next JSON-RPC message.
    pub async fn read_message(&mut self) -> Result<LspMessage> {
        // Read headers until empty line
        let mut content_length: Option<usize> = None;
        let mut header_line = String::new();

        loop {
            header_line.clear();
            let bytes_read = self.reader.read_line(&mut header_line).await
                .context("Failed to read LSP header")?;
            if bytes_read == 0 {
                anyhow::bail!("LSP server closed stdout");
            }

            let trimmed = header_line.trim();
            if trimmed.is_empty() {
                break; // End of headers
            }

            if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
                content_length = Some(len_str.parse().context("Invalid Content-Length")?);
            }
            // Also handle lowercase (some servers use it)
            if let Some(len_str) = trimmed.strip_prefix("content-length: ") {
                content_length = Some(len_str.parse().context("Invalid Content-Length")?);
            }
        }

        let length = content_length.context("Missing Content-Length header")?;

        // Read exactly `length` bytes for the body
        let mut body = vec![0u8; length];
        tokio::io::AsyncReadExt::read_exact(&mut self.reader, &mut body).await
            .context("Failed to read LSP message body")?;

        let json: serde_json::Value = serde_json::from_slice(&body)
            .context("Failed to parse LSP message as JSON")?;

        // Classify the message
        if let Some(id) = json.get("id").and_then(|v| v.as_i64()) {
            if json.get("error").is_some() {
                Ok(LspMessage::Error {
                    id,
                    error: json["error"].clone(),
                })
            } else {
                Ok(LspMessage::Response {
                    id,
                    result: json.get("result").cloned().unwrap_or(serde_json::Value::Null),
                })
            }
        } else {
            // Notification (no id)
            Ok(LspMessage::Notification {
                method: json["method"].as_str().unwrap_or("").to_string(),
                params: json.get("params").cloned().unwrap_or(serde_json::Value::Null),
            })
        }
    }
}
