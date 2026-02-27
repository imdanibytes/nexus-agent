use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use super::config::LspServerConfig;
use super::diagnostics::DiagnosticCache;
use super::protocol::{LspMessage, LspReader, LspWriter};

/// Requests sent to the background LSP service task.
#[allow(dead_code)]
enum LspRequest {
    DidOpen {
        uri: String,
        language_id: String,
        version: i32,
        text: String,
        reply: oneshot::Sender<Result<()>>,
    },
    DidChange {
        uri: String,
        version: i32,
        text: String,
        reply: oneshot::Sender<Result<()>>,
    },
    DidClose {
        uri: String,
        reply: oneshot::Sender<Result<()>>,
    },
    Shutdown(oneshot::Sender<()>),
}

/// A running LSP server process for a specific project root.
#[allow(dead_code)]
pub struct LspServer {
    pub id: String,
    pub name: String,
    pub language_ids: Vec<String>,
    tx: mpsc::Sender<LspRequest>,
    pub diagnostics: Arc<DiagnosticCache>,
    /// Tracks which files have been opened (URI → version counter).
    open_docs: tokio::sync::RwLock<HashMap<String, i32>>,
}

impl LspServer {
    /// Spawn an LSP server process, perform the initialize handshake,
    /// and start the background message loop.
    pub async fn spawn(config: &LspServerConfig, root_uri: &str) -> Result<Self> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .with_context(|| format!("Failed to spawn LSP server '{}'", config.name))?;

        let stdin = child.stdin.take().context("LSP stdin not available")?;
        let stdout = child.stdout.take().context("LSP stdout not available")?;

        let mut writer = LspWriter::new(stdin);
        let mut reader = LspReader::new(stdout);

        // Send initialize request
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {
                        "relatedInformation": true
                    },
                    "synchronization": {
                        "didOpen": true,
                        "didChange": true,
                        "didClose": true
                    }
                }
            }
        });

        let init_id = writer.request("initialize", init_params).await
            .context("Failed to send initialize")?;

        // Wait for initialize response (with timeout — some servers are slow to init)
        let init_result = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let msg = reader.read_message().await
                    .context("Failed reading initialize response")?;
                match msg {
                    LspMessage::Response { id, .. } if id == init_id => break Ok::<_, anyhow::Error>(()),
                    LspMessage::Error { id, error } if id == init_id => {
                        anyhow::bail!("LSP initialize failed: {error}");
                    }
                    _ => continue, // Skip notifications during init
                }
            }
        }).await;

        match init_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => anyhow::bail!("LSP initialize timed out after 30s"),
        }

        // Send initialized notification
        writer.notify("initialized", serde_json::json!({})).await?;

        let diagnostics = DiagnosticCache::new();
        let diagnostics_clone = Arc::clone(&diagnostics);

        let (tx, rx) = mpsc::channel::<LspRequest>(64);

        // Spawn background task
        let server_name = config.name.clone();
        tokio::spawn(async move {
            Self::background_loop(reader, writer, rx, diagnostics_clone, child, &server_name).await;
        });

        Ok(Self {
            id: config.id.clone(),
            name: config.name.clone(),
            language_ids: config.language_ids.clone(),
            tx,
            diagnostics,
            open_docs: tokio::sync::RwLock::new(HashMap::new()),
        })
    }

    /// Check if the background loop has exited (server crashed or was killed).
    pub fn is_dead(&self) -> bool {
        self.tx.is_closed()
    }

    /// Open a file in the LSP server (sends textDocument/didOpen).
    /// Reads file content from disk if not provided.
    pub async fn open_file(&self, path: &str, language_id: &str) -> Result<()> {
        let uri = format!("file://{path}");

        // Check if already open
        {
            let docs = self.open_docs.read().await;
            if docs.contains_key(&uri) {
                return Ok(());
            }
        }

        let text = tokio::fs::read_to_string(path).await
            .with_context(|| format!("Failed to read {path} for LSP didOpen"))?;

        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(LspRequest::DidOpen {
            uri: uri.clone(),
            language_id: language_id.to_string(),
            version: 1,
            text,
            reply: reply_tx,
        }).await.ok();

        if let Ok(result) = reply_rx.await {
            result?;
        }

        let mut docs = self.open_docs.write().await;
        docs.insert(uri, 1);
        Ok(())
    }

    /// Notify LSP that a file changed (sends textDocument/didChange with full content).
    pub async fn notify_change(&self, path: &str, content: &str) -> Result<()> {
        let uri = format!("file://{path}");

        let version = {
            let mut docs = self.open_docs.write().await;
            let v = docs.entry(uri.clone()).or_insert(0);
            *v += 1;
            *v
        };

        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(LspRequest::DidChange {
            uri,
            version,
            text: content.to_string(),
            reply: reply_tx,
        }).await.ok();

        if let Ok(result) = reply_rx.await {
            result?;
        }
        Ok(())
    }

    /// Get diagnostics, waiting briefly for fresh ones if needed.
    pub async fn diagnostics_for(&self, path: &str, timeout: Duration) -> super::diagnostics::DiagnosticStatus {
        let uri = format!("file://{path}");
        self.diagnostics.wait_for(&uri, timeout).await
    }

    /// Get cached diagnostics (non-blocking).
    pub async fn cached_diagnostics(&self, path: &str) -> Vec<lsp_types::Diagnostic> {
        let uri = format!("file://{path}");
        self.diagnostics.get(&uri).await
    }

    /// Shutdown the LSP server gracefully.
    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(LspRequest::Shutdown(tx)).await;
        let _ = tokio::time::timeout(Duration::from_secs(5), rx).await;
    }

    /// Background loop: reads LSP messages and processes requests.
    ///
    /// NOTE: We do NOT use tokio::select! for reading because
    /// `LspReader::read_message()` is NOT cancellation safe (it uses
    /// `BufReader::read_line` which loses data on cancel). Instead, we
    /// use biased select with reading as the primary branch.
    async fn background_loop(
        mut reader: LspReader,
        mut writer: LspWriter,
        mut rx: mpsc::Receiver<LspRequest>,
        diagnostics: Arc<DiagnosticCache>,
        mut child: Child,
        server_name: &str,
    ) {
        tracing::debug!(server = %server_name, "Background loop started");
        loop {
            tokio::select! {
                biased;
                // Read incoming LSP message (biased = priority over outgoing)
                msg_result = reader.read_message() => {
                    match msg_result {
                        Ok(LspMessage::Notification { method, params }) => {
                            if method == "textDocument/publishDiagnostics" {
                                if let (Some(uri), Some(diags)) = (
                                    params.get("uri").and_then(|u| u.as_str()),
                                    params.get("diagnostics"),
                                ) {
                                    let diags: Vec<lsp_types::Diagnostic> =
                                        serde_json::from_value(diags.clone()).unwrap_or_default();
                                    let version = params.get("version").and_then(|v| v.as_i64()).map(|v| v as i32);
                                    tracing::debug!(
                                        server = %server_name,
                                        %uri,
                                        count = diags.len(),
                                        "publishDiagnostics received"
                                    );
                                    diagnostics.update(uri, diags, version).await;
                                }
                            }
                        }
                        Ok(LspMessage::Response { id, .. }) => {
                            tracing::trace!(server = %server_name, id, "LSP response");
                        }
                        Ok(LspMessage::Error { id, error }) => {
                            tracing::debug!(server = %server_name, id, %error, "LSP error response");
                        }
                        Err(e) => {
                            tracing::warn!(server = %server_name, error = %e, "LSP read error, shutting down");
                            break;
                        }
                    }
                }

                // Process outgoing requests
                Some(req) = rx.recv() => {
                    match req {
                        LspRequest::DidOpen { uri, language_id, version, text, reply } => {
                            let params = serde_json::json!({
                                "textDocument": {
                                    "uri": uri,
                                    "languageId": language_id,
                                    "version": version,
                                    "text": text,
                                }
                            });
                            let result = writer.notify("textDocument/didOpen", params).await;
                            let _ = reply.send(result);
                        }
                        LspRequest::DidChange { uri, version, text, reply } => {
                            let params = serde_json::json!({
                                "textDocument": { "uri": uri, "version": version },
                                "contentChanges": [{ "text": text }]
                            });
                            let result = writer.notify("textDocument/didChange", params).await;
                            let _ = reply.send(result);
                        }
                        LspRequest::DidClose { uri, reply } => {
                            let params = serde_json::json!({
                                "textDocument": { "uri": uri }
                            });
                            let result = writer.notify("textDocument/didClose", params).await;
                            let _ = reply.send(result);
                        }
                        LspRequest::Shutdown(reply) => {
                            let _ = writer.request("shutdown", serde_json::Value::Null).await;
                            let _ = writer.notify("exit", serde_json::Value::Null).await;
                            let _ = child.kill().await;
                            let _ = reply.send(());
                            break;
                        }
                    }
                }
            }
        }
    }
}
