//! Daemon introspection MCP server.
//!
//! Lightweight, localhost-only MCP server exposing read-only tools for
//! querying the running Nexus daemon state: conversations, agents, providers,
//! MCP connections, task state, and active turns.
//!
//! Mounted at `/mcp` on the existing Axum router.

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::json;

use super::AppState;

// ---------------------------------------------------------------------------
// Tool catalog
// ---------------------------------------------------------------------------

struct ToolDef {
    name: &'static str,
    description: &'static str,
    schema: serde_json::Value,
}

fn tool_defs() -> &'static [ToolDef] {
    static TOOLS: std::sync::OnceLock<Vec<ToolDef>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(|| vec![
        ToolDef {
            name: "list_conversations",
            description: "List all conversations with metadata (id, title, timestamps, message_count).",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDef {
            name: "get_conversation",
            description: "Get a full conversation including messages, active_path, usage, and task state.",
            schema: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
                "additionalProperties": false,
            }),
        },
        ToolDef {
            name: "list_agents",
            description: "List all configured agent entries (id, name, provider, model, settings).",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDef {
            name: "get_active_agent",
            description: "Get the currently active agent entry, or null if none is set.",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDef {
            name: "list_mcp_servers",
            description: "List configured MCP server entries (id, name, command, args, env).",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDef {
            name: "list_tools",
            description: "List all available tools: MCP tools (namespaced) and built-in tools with their descriptions.",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDef {
            name: "get_task_state",
            description: "Get the task/plan state for a conversation (plan text, tasks, mode).",
            schema: json!({
                "type": "object",
                "properties": { "conversation_id": { "type": "string" } },
                "required": ["conversation_id"],
                "additionalProperties": false,
            }),
        },
        ToolDef {
            name: "server_status",
            description: "Get daemon summary: active turn count, conversation count, MCP server count, agent count.",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDef {
            name: "list_providers",
            description: "List configured providers (id, name, type, endpoint, region — no secrets).",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDef {
            name: "send_message",
            description: "Send a user message to a conversation, starting a new turn.",
            schema: json!({
                "type": "object",
                "properties": {
                    "conversation_id": { "type": "string" },
                    "message": { "type": "string" },
                },
                "required": ["conversation_id", "message"],
                "additionalProperties": false,
            }),
        },
        ToolDef {
            name: "abort_turn",
            description: "Abort the active turn for a conversation.",
            schema: json!({
                "type": "object",
                "properties": { "conversation_id": { "type": "string" } },
                "required": ["conversation_id"],
                "additionalProperties": false,
            }),
        },
        ToolDef {
            name: "list_mcp_resources",
            description: "List resources exposed by connected MCP servers, grouped by server ID.",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
    ])
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

pub struct IntrospectMcpServer {
    state: Arc<AppState>,
}

impl IntrospectMcpServer {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    // -- Read-only tools -----------------------------------------------------

    async fn handle_list_conversations(&self) -> Result<CallToolResult, McpError> {
        let threads = self.state.threads.list().await;
        ok_json(&threads)
    }

    async fn handle_get_conversation(&self, args: &serde_json::Map<String, serde_json::Value>) -> Result<CallToolResult, McpError> {
        let id = require_str(args, "id")?;
        match self.state.threads.get(&id).await {
            Ok(Some(conv)) => ok_json(&conv),
            Ok(None) => Err(McpError::invalid_params(format!("conversation '{}' not found", id), None)),
            Err(e) => Err(McpError::internal_error(e.to_string(), None)),
        }
    }

    async fn handle_list_agents(&self) -> Result<CallToolResult, McpError> {
        let store = self.state.agents.agents.read().await;
        ok_json(&store.list())
    }

    async fn handle_get_active_agent(&self) -> Result<CallToolResult, McpError> {
        let store = self.state.agents.agents.read().await;
        let active = store
            .active_agent_id()
            .and_then(|id| store.get(id));
        ok_json(&active)
    }

    async fn handle_list_mcp_servers(&self) -> Result<CallToolResult, McpError> {
        let store = self.state.mcp.configs.read().await;
        ok_json(&store.list())
    }

    async fn handle_list_tools(&self) -> Result<CallToolResult, McpError> {
        let mcp = self.state.mcp.mcp.read().await;
        let mcp_tools: Vec<serde_json::Value> = mcp.tools().iter().map(|t| {
            json!({ "name": t.name, "description": t.description, "source": "mcp" })
        }).collect();
        ok_json(&mcp_tools)
    }

    async fn handle_get_task_state(&self, args: &serde_json::Map<String, serde_json::Value>) -> Result<CallToolResult, McpError> {
        let conv_id = require_str(args, "conversation_id")?;
        let mut store = self.state.chat.task_store.write().await;
        match store.get(&conv_id) {
            Some(state) => ok_json(&state),
            None => ok_json(&serde_json::Value::Null),
        }
    }

    async fn handle_server_status(&self) -> Result<CallToolResult, McpError> {
        let active_turns = self.state.chat.active_turns.lock().await.len();
        let conversations = self.state.threads.list().await.len();
        let mcp_servers = self.state.mcp.configs.read().await.list().len();
        let mcp_tools = self.state.mcp.mcp.read().await.tools().len();
        let agents = self.state.agents.agents.read().await.list().len();
        let providers = self.state.agents.providers.read().await.list().len();

        ok_json(&json!({
            "active_turns": active_turns,
            "conversations": conversations,
            "mcp_servers": mcp_servers,
            "mcp_tools": mcp_tools,
            "agents": agents,
            "providers": providers,
        }))
    }

    async fn handle_list_providers(&self) -> Result<CallToolResult, McpError> {
        let store = self.state.agents.providers.read().await;
        let public: Vec<crate::provider::ProviderPublic> =
            store.list().iter().map(|p| p.into()).collect();
        ok_json(&public)
    }

    async fn handle_list_mcp_resources(&self) -> Result<CallToolResult, McpError> {
        let mcp = self.state.mcp.mcp.read().await;
        let grouped = mcp.all_resources().await;
        let result: Vec<serde_json::Value> = grouped
            .into_iter()
            .map(|(server_id, resources)| {
                let res: Vec<serde_json::Value> = resources
                    .iter()
                    .map(|r| {
                        json!({
                            "uri": r.uri.as_str(),
                            "name": r.name,
                            "description": r.description,
                            "mime_type": r.mime_type,
                        })
                    })
                    .collect();
                json!({ "server_id": server_id, "resources": res })
            })
            .collect();
        ok_json(&result)
    }

    // -- Write tools ---------------------------------------------------------

    async fn handle_send_message(&self, args: &serde_json::Map<String, serde_json::Value>) -> Result<CallToolResult, McpError> {
        let conv_id = require_str(args, "conversation_id")?;
        let message = require_str(args, "message")?;

        use super::message_queue::QueuedMessage;
        self.state.chat.message_queue.enqueue(&conv_id, QueuedMessage {
            text: message,
            metadata: serde_json::Value::Null,
        }).await;

        ok_json(&json!({ "ok": true, "conversation_id": conv_id }))
    }

    async fn handle_abort_turn(&self, args: &serde_json::Map<String, serde_json::Value>) -> Result<CallToolResult, McpError> {
        let conv_id = require_str(args, "conversation_id")?;

        let mut active = self.state.chat.active_turns.lock().await;
        if let Some(turn) = active.remove(&conv_id) {
            turn.cancel.cancel();
            ok_json(&json!({ "ok": true, "aborted_run": turn.run_id }))
        } else {
            ok_json(&json!({ "ok": false, "reason": "no active turn" }))
        }
    }
}

// ---------------------------------------------------------------------------
// ServerHandler
// ---------------------------------------------------------------------------

impl ServerHandler for IntrospectMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "nexus-introspect".into(),
                version: "0.1.0".into(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Nexus daemon introspection — query conversations, agents, providers, MCP servers, and task state."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = tool_defs()
            .iter()
            .map(|def| Tool {
                name: Cow::Borrowed(def.name),
                title: None,
                description: Some(Cow::Borrowed(def.description)),
                input_schema: Arc::new(
                    def.schema.as_object().cloned().unwrap_or_default(),
                ),
                output_schema: None,
                annotations: None,
                execution: None,
                icons: None,
                meta: None,
            })
            .collect();
        Ok(ListToolsResult { tools, next_cursor: None, meta: None })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let args = request.arguments.unwrap_or_default();
        match request.name.as_ref() {
            "list_conversations" => self.handle_list_conversations().await,
            "get_conversation" => self.handle_get_conversation(&args).await,
            "list_agents" => self.handle_list_agents().await,
            "get_active_agent" => self.handle_get_active_agent().await,
            "list_mcp_servers" => self.handle_list_mcp_servers().await,
            "list_tools" => self.handle_list_tools().await,
            "get_task_state" => self.handle_get_task_state(&args).await,
            "server_status" => self.handle_server_status().await,
            "list_providers" => self.handle_list_providers().await,
            "send_message" => self.handle_send_message(&args).await,
            "abort_turn" => self.handle_abort_turn(&args).await,
            "list_mcp_resources" => self.handle_list_mcp_resources().await,
            _ => Err(McpError::invalid_request(format!("unknown tool: {}", request.name), None)),
        }
    }
}

// ---------------------------------------------------------------------------
// Re-exports for inline construction in mod.rs
// ---------------------------------------------------------------------------

pub use rmcp::transport::streamable_http_server::session::local::LocalSessionManager
    as IntrospectSessionManager;
pub use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig as IntrospectServerConfig,
    StreamableHttpService as IntrospectService,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ok_json(value: &impl serde::Serialize) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string()),
    )]))
}

fn require_str(args: &serde_json::Map<String, serde_json::Value>, key: &str) -> Result<String, McpError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| McpError::invalid_params(format!("missing {}", key), None))
}
