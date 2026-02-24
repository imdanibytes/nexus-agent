pub(crate) mod server;
pub mod store;

use std::collections::HashMap;
use std::collections::HashSet;

use crate::anthropic::types::Tool as AnthropicTool;
use crate::config::McpServerConfig;
use server::McpServer;

/// Manages all MCP server connections, routes tool calls.
pub struct McpManager {
    servers: Vec<McpServer>,
    /// Maps tool_name → server index in `servers`
    tool_routing: HashMap<String, usize>,
}

impl McpManager {
    /// Spawn and connect to all configured MCP servers.
    pub async fn from_configs(configs: &[McpServerConfig]) -> Self {
        let mut servers = Vec::new();
        let mut tool_routing = HashMap::new();

        for config in configs {
            match McpServer::spawn(config).await {
                Ok(srv) => {
                    let idx = servers.len();
                    for tool in srv.tools() {
                        let name = tool.name.to_string();
                        if tool_routing.contains_key(&name) {
                            let prefixed = format!("{}__{}", config.id, name);
                            tracing::warn!(
                                tool = %name,
                                server = %config.id,
                                prefixed = %prefixed,
                                "Tool name conflict, prefixing"
                            );
                            tool_routing.insert(prefixed, idx);
                        } else {
                            tool_routing.insert(name, idx);
                        }
                    }
                    servers.push(srv);
                }
                Err(e) => {
                    tracing::error!(server = %config.id, error = %e, "Failed to start MCP server");
                }
            }
        }

        tracing::info!(
            servers = servers.len(),
            tools = tool_routing.len(),
            "MCP manager initialized"
        );

        Self {
            servers,
            tool_routing,
        }
    }

    /// Get all tools as Anthropic API Tool definitions.
    pub fn tools(&self) -> Vec<AnthropicTool> {
        self.tools_inner(None)
    }

    /// Get tools filtered to only the given MCP server IDs.
    /// If `server_ids` is None, returns all tools.
    /// If `server_ids` is Some([]), returns no tools.
    pub fn tools_for(&self, server_ids: Option<&[String]>) -> Vec<AnthropicTool> {
        self.tools_inner(server_ids)
    }

    fn tools_inner(&self, server_ids: Option<&[String]>) -> Vec<AnthropicTool> {
        let allowed: Option<HashSet<&str>> = server_ids.map(|ids| ids.iter().map(|s| s.as_str()).collect());

        let mut result = Vec::new();

        for (routed_name, &server_idx) in &self.tool_routing {
            let server = &self.servers[server_idx];

            // Filter by allowed server IDs if specified
            if let Some(ref allowed_set) = allowed {
                if !allowed_set.contains(server.id.as_str()) {
                    continue;
                }
            }

            let original_name = routed_name
                .strip_prefix(&format!("{server_id}__", server_id = server.id))
                .unwrap_or(routed_name);

            if let Some(tool) = server.tools().iter().find(|t| t.name == original_name) {
                let schema = serde_json::to_value(&tool.input_schema)
                    .unwrap_or_else(|_| serde_json::json!({"type": "object"}));

                result.push(AnthropicTool {
                    name: routed_name.clone(),
                    description: tool
                        .description
                        .as_deref()
                        .unwrap_or("")
                        .to_string(),
                    input_schema: schema,
                });
            }
        }

        result
    }

    /// Call a tool by name, routing to the correct MCP server.
    pub async fn call_tool(
        &self,
        name: &str,
        args_json: &str,
    ) -> (String, bool) {
        let Some(&server_idx) = self.tool_routing.get(name) else {
            return (
                format!("Unknown tool '{}'. No MCP server provides this tool.", name),
                true,
            );
        };

        let server = &self.servers[server_idx];

        let original_name = name
            .strip_prefix(&format!("{server_id}__", server_id = server.id))
            .unwrap_or(name);

        let arguments: Option<serde_json::Map<String, serde_json::Value>> =
            match serde_json::from_str(args_json) {
                Ok(serde_json::Value::Object(map)) => Some(map),
                _ => Some(serde_json::Map::new()),
            };

        match server.call_tool(original_name, arguments).await {
            Ok(result) => {
                let is_error = result.is_error.unwrap_or(false);
                let text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                    .collect::<Vec<_>>()
                    .join("\n");
                (text, is_error)
            }
            Err(e) => (format!("Tool call failed: {}", e), true),
        }
    }

    /// Shut down all MCP servers.
    pub async fn shutdown(&self) {
        for server in &self.servers {
            server.shutdown().await;
        }
    }
}
