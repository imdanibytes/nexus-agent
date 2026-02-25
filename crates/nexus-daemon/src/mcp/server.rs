use anyhow::{Context, Result};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::ServiceExt;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use std::collections::HashMap;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use crate::config::McpServerConfig;

/// Requests sent to the background service task.
enum McpRequest {
    CallTool {
        name: String,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        reply: oneshot::Sender<Result<CallToolResult>>,
    },
    Shutdown(oneshot::Sender<()>),
}

/// A connected MCP server process.
/// The actual service runs in a background task; we communicate via channels.
pub struct McpServer {
    pub id: String,
    pub name: String,
    tools: Vec<rmcp::model::Tool>,
    tx: mpsc::Sender<McpRequest>,
}

impl McpServer {
    /// Spawn an MCP server from config, connect, and discover tools.
    pub async fn spawn(config: &McpServerConfig) -> Result<Self> {
        let args = config.args.clone();
        let env: HashMap<String, String> = config.env.clone();

        let transport = TokioChildProcess::new(
            Command::new(&config.command).configure(move |cmd| {
                cmd.args(&args);
                for (k, v) in &env {
                    cmd.env(k, v);
                }
            }),
        )
        .with_context(|| format!("Failed to spawn MCP server '{}'", config.id))?;

        let service = ().serve(transport).await.with_context(|| {
            format!("Failed to initialize MCP server '{}'", config.id)
        })?;

        let tools_result = service.list_tools(Default::default()).await.with_context(|| {
            format!("Failed to list tools from MCP server '{}'", config.id)
        })?;

        let tools = tools_result.tools;

        tracing::info!(
            server = %config.id,
            tool_count = tools.len(),
            "MCP server connected"
        );

        for tool in &tools {
            tracing::debug!(server = %config.id, tool = %tool.name, "  Tool discovered");
        }

        // Spawn background task that owns the service
        let (tx, mut rx) = mpsc::channel::<McpRequest>(32);
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                match req {
                    McpRequest::CallTool {
                        name,
                        arguments,
                        reply,
                    } => {
                        let result = service
                            .call_tool(CallToolRequestParams {
                                name: name.into(),
                                arguments,
                                meta: None,
                                task: None,
                            })
                            .await
                            .map_err(|e| anyhow::anyhow!("{}", e));
                        let _ = reply.send(result);
                    }
                    McpRequest::Shutdown(reply) => {
                        let _ = service.cancel().await;
                        let _ = reply.send(());
                        break;
                    }
                }
            }
        });

        Ok(Self {
            id: config.id.clone(),
            name: config.name.clone(),
            tools,
            tx,
        })
    }

    /// Get the tools this server provides.
    pub fn tools(&self) -> &[rmcp::model::Tool] {
        &self.tools
    }

    /// Call a tool on this server.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<CallToolResult> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpRequest::CallTool {
                name: name.to_string(),
                arguments,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' is not running", self.id))?;

        reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' dropped the response", self.id))?
    }

    /// Shut down the MCP server.
    pub async fn shutdown(&self) {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.tx.send(McpRequest::Shutdown(reply_tx)).await.is_ok() {
            let _ = reply_rx.await;
        }
    }
}
