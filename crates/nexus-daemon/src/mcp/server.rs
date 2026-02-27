use anyhow::{Context, Result};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
    ReadResourceRequestParams, ReadResourceResult, Resource,
};
use rmcp::service::{RunningService, Service, ServiceExt};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::RoleClient;
use std::collections::HashMap;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use crate::config::McpServerConfig;
use super::handler::{ClientHandlerState, NexusClientHandler};

/// Requests sent to the background service task.
enum McpRequest {
    CallTool {
        name: String,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        reply: oneshot::Sender<Result<CallToolResult>>,
    },
    ListResources {
        reply: oneshot::Sender<Result<Vec<Resource>>>,
    },
    ReadResource {
        uri: String,
        reply: oneshot::Sender<Result<ReadResourceResult>>,
    },
    #[allow(dead_code)] // plumbed for future live re-fetch
    ListPrompts {
        reply: oneshot::Sender<Result<Vec<rmcp::model::Prompt>>>,
    },
    GetPrompt {
        name: String,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        reply: oneshot::Sender<Result<GetPromptResult>>,
    },
    Shutdown(oneshot::Sender<()>),
}

/// A connected MCP server process.
/// The actual service runs in a background task; we communicate via channels.
pub struct McpServer {
    pub id: String,
    #[allow(dead_code)] // stored for display/logging purposes
    pub name: String,
    tools: Vec<rmcp::model::Tool>,
    prompts: Vec<rmcp::model::Prompt>,
    tx: mpsc::Sender<McpRequest>,
}

impl McpServer {
    /// Spawn an MCP server from config, connect, and discover tools.
    ///
    /// If the config has a `url`, connects via streamable HTTP.
    /// Otherwise, spawns a child process via stdio.
    pub async fn spawn(config: &McpServerConfig, handler_state: &ClientHandlerState) -> Result<Self> {
        let handler = NexusClientHandler::new(handler_state.clone());

        if let Some(url) = &config.url {
            // HTTP transport
            let mut http_config = StreamableHttpClientTransportConfig::with_uri(url.as_str());

            // Apply custom headers if configured
            if let Some(headers) = &config.headers {
                for (key, value) in headers {
                    let name: http::HeaderName = key.parse().with_context(|| {
                        format!("Invalid header name '{}' for MCP server '{}'", key, config.id)
                    })?;
                    let val: http::HeaderValue = value.parse().with_context(|| {
                        format!("Invalid header value for '{}' on MCP server '{}'", key, config.id)
                    })?;
                    http_config.custom_headers.insert(name, val);
                }
            }

            let transport = StreamableHttpClientTransport::<reqwest::Client>::with_client(
                reqwest::Client::new(),
                http_config,
            );
            let service = handler.serve(transport).await.with_context(|| {
                format!("Failed to connect to MCP server '{}' at {}", config.id, url)
            })?;

            Self::finish_spawn(service, config).await
        } else {
            // Stdio transport
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

            let service = handler.serve(transport).await.with_context(|| {
                format!("Failed to initialize MCP server '{}'", config.id)
            })?;

            Self::finish_spawn(service, config).await
        }
    }

    /// Common post-connection logic: list tools, spawn background task.
    async fn finish_spawn<S: Service<RoleClient>>(
        service: RunningService<RoleClient, S>,
        config: &McpServerConfig,
    ) -> Result<Self> {
        let tools_result = service.list_tools(Default::default()).await.with_context(|| {
            format!("Failed to list tools from MCP server '{}'", config.id)
        })?;

        let tools = tools_result.tools;

        // Discover prompts (best-effort — not all servers support prompts)
        let prompts = match service.list_all_prompts().await {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(server = %config.id, error = %e, "No prompts from MCP server");
                Vec::new()
            }
        };

        tracing::info!(
            server = %config.id,
            tool_count = tools.len(),
            prompt_count = prompts.len(),
            "MCP server connected"
        );

        for tool in &tools {
            tracing::debug!(server = %config.id, tool = %tool.name, "  Tool discovered");
        }
        for prompt in &prompts {
            tracing::debug!(server = %config.id, prompt = %prompt.name, "  Prompt discovered");
        }

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
                    McpRequest::ListResources { reply } => {
                        let result = service
                            .list_all_resources()
                            .await
                            .map_err(|e| anyhow::anyhow!("{}", e));
                        let _ = reply.send(result);
                    }
                    McpRequest::ReadResource { uri, reply } => {
                        let result = service
                            .read_resource(ReadResourceRequestParams {
                                uri,
                                meta: None,
                            })
                            .await
                            .map_err(|e| anyhow::anyhow!("{}", e));
                        let _ = reply.send(result);
                    }
                    McpRequest::ListPrompts { reply } => {
                        let result = service
                            .list_all_prompts()
                            .await
                            .map_err(|e| anyhow::anyhow!("{}", e));
                        let _ = reply.send(result);
                    }
                    McpRequest::GetPrompt { name, arguments, reply } => {
                        let result = service
                            .get_prompt(GetPromptRequestParams {
                                name: name.into(),
                                arguments,
                                meta: None,
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
            prompts,
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

    /// List all resources exposed by this server.
    pub async fn list_resources(&self) -> Result<Vec<Resource>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpRequest::ListResources { reply: reply_tx })
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' is not running", self.id))?;

        reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' dropped the response", self.id))?
    }

    /// Read a specific resource by URI from this server.
    pub async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpRequest::ReadResource {
                uri: uri.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' is not running", self.id))?;

        reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' dropped the response", self.id))?
    }

    /// Get the prompts this server provides.
    pub fn prompts(&self) -> &[rmcp::model::Prompt] {
        &self.prompts
    }

    /// List prompts from this server (re-fetches from the running server).
    #[allow(dead_code)] // plumbed for future live re-fetch
    pub async fn list_prompts(&self) -> Result<Vec<rmcp::model::Prompt>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpRequest::ListPrompts { reply: reply_tx })
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' is not running", self.id))?;

        reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' dropped the response", self.id))?
    }

    /// Get a specific prompt by name, optionally with arguments.
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<GetPromptResult> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(McpRequest::GetPrompt {
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
