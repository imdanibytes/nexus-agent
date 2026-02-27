pub mod handler;
pub(crate) mod server;
pub mod store;

use std::collections::HashMap;
use std::collections::HashSet;

use crate::anthropic::types::Tool as AnthropicTool;
use crate::config::McpServerConfig;
pub use handler::ClientHandlerState;
use server::McpServer;

/// Manages all MCP server connections, routes tool calls.
pub struct McpManager {
    servers: Vec<McpServer>,
    /// Maps namespaced tool name → (server index, original tool name)
    tool_routing: HashMap<String, (usize, String)>,
}

/// Sanitize a server name into a valid tool name component.
/// Lowercase, replace non-alphanumeric with underscore, collapse runs.
fn sanitize_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    // Collapse consecutive underscores
    let mut result = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for c in s.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push(c);
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }
    result.trim_matches('_').to_string()
}

impl McpManager {
    /// Spawn and connect to all configured MCP servers.
    ///
    /// All MCP tools are namespaced as `mcp_<server_name>__<tool_name>` to
    /// avoid collisions with built-in tools or other MCP servers.
    pub async fn from_configs(configs: &[McpServerConfig], handler_state: &ClientHandlerState) -> Self {
        let mut servers = Vec::new();
        let mut tool_routing = HashMap::new();

        for config in configs {
            match McpServer::spawn(config, handler_state).await {
                Ok(srv) => {
                    let idx = servers.len();
                    let prefix = sanitize_name(&config.name);

                    for tool in srv.tools() {
                        let original = tool.name.to_string();
                        let namespaced = format!("mcp_{prefix}__{original}");

                        match tool_routing.entry(namespaced) {
                            std::collections::hash_map::Entry::Occupied(e) => {
                                tracing::warn!(
                                    tool = %original,
                                    namespaced = %e.key(),
                                    server = %config.name,
                                    "Duplicate namespaced tool name, skipping"
                                );
                            }
                            std::collections::hash_map::Entry::Vacant(e) => {
                                e.insert((idx, original));
                            }
                        }
                    }
                    servers.push(srv);
                }
                Err(e) => {
                    tracing::error!(server = %config.name, error = %e, "Failed to start MCP server");
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
        let allowed: Option<HashSet<&str>> =
            server_ids.map(|ids| ids.iter().map(|s| s.as_str()).collect());

        let mut result = Vec::new();

        for (namespaced_name, (server_idx, original_name)) in &self.tool_routing {
            let server = &self.servers[*server_idx];

            if let Some(ref allowed_set) = allowed {
                if !allowed_set.contains(server.id.as_str()) {
                    continue;
                }
            }

            if let Some(tool) = server.tools().iter().find(|t| t.name == *original_name) {
                let schema = serde_json::to_value(&tool.input_schema)
                    .unwrap_or_else(|_| serde_json::json!({"type": "object"}));

                result.push(AnthropicTool {
                    name: namespaced_name.clone(),
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

    /// Call a tool by its namespaced name, routing to the correct MCP server.
    pub async fn call_tool(
        &self,
        name: &str,
        args_json: &str,
    ) -> (String, bool) {
        let Some((server_idx, original_name)) = self.tool_routing.get(name) else {
            return (
                format!("Unknown tool '{}'. No MCP server provides this tool.", name),
                true,
            );
        };

        let server = &self.servers[*server_idx];

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

    /// List resources from a specific MCP server.
    pub async fn list_resources(&self, server_id: &str) -> anyhow::Result<Vec<rmcp::model::Resource>> {
        let server = self.servers.iter().find(|s| s.id == server_id);
        match server {
            Some(s) => s.list_resources().await,
            None => anyhow::bail!("No MCP server with id '{}'", server_id),
        }
    }

    /// Read a resource by URI from a specific MCP server.
    pub async fn read_resource(
        &self,
        server_id: &str,
        uri: &str,
    ) -> anyhow::Result<rmcp::model::ReadResourceResult> {
        let server = self.servers.iter().find(|s| s.id == server_id);
        match server {
            Some(s) => s.read_resource(uri).await,
            None => anyhow::bail!("No MCP server with id '{}'", server_id),
        }
    }

    /// List resources from all servers, grouped by server ID.
    pub async fn all_resources(&self) -> Vec<(String, Vec<rmcp::model::Resource>)> {
        let mut results = Vec::new();
        for server in &self.servers {
            match server.list_resources().await {
                Ok(resources) if !resources.is_empty() => {
                    results.push((server.id.clone(), resources));
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!(server = %server.id, error = %e, "Failed to list resources");
                }
            }
        }
        results
    }

    /// Shut down all MCP servers.
    pub async fn shutdown(&self) {
        for server in &self.servers {
            server.shutdown().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_name_basic() {
        assert_eq!(sanitize_name("filesystem"), "filesystem");
        assert_eq!(sanitize_name("My Server"), "my_server");
        assert_eq!(sanitize_name("git-hub"), "git_hub");
        assert_eq!(sanitize_name("  spaces  "), "spaces");
        assert_eq!(sanitize_name("A--B__C"), "a_b_c");
    }
}
