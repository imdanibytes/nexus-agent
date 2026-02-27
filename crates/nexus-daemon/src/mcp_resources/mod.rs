use nexus_provider::types::Tool;
use crate::mcp::McpManager;

const LIST_TOOL: &str = "mcp_list_resources";
const READ_TOOL: &str = "mcp_read_resource";

pub fn is_resource_tool(name: &str) -> bool {
    name == LIST_TOOL || name == READ_TOOL
}

pub fn tool_definitions() -> Vec<Tool> {
    vec![list_definition(), read_definition()]
}

fn list_definition() -> Tool {
    Tool {
        name: LIST_TOOL.to_string(),
        description: "Lists resources available from connected MCP servers. Resources are \
            server-provided data like database schemas, documentation, config files, etc. \
            Use this to discover what resources are available before reading them."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "server_id": {
                    "type": "string",
                    "description": "Optional MCP server ID to filter resources. Omit to list from all servers."
                }
            }
        }),
    }
}

fn read_definition() -> Tool {
    Tool {
        name: READ_TOOL.to_string(),
        description: "Reads a specific resource from an MCP server by its URI. \
            Use mcp_list_resources first to discover available resources and their URIs."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "server_id": {
                    "type": "string",
                    "description": "MCP server ID that hosts the resource."
                },
                "uri": {
                    "type": "string",
                    "description": "Resource URI to read (from mcp_list_resources output)."
                }
            },
            "required": ["server_id", "uri"]
        }),
    }
}

#[derive(serde::Deserialize)]
struct ListArgs {
    server_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct ReadArgs {
    server_id: String,
    uri: String,
}

/// Execute a resource tool call. Returns (content, is_error).
pub async fn execute(
    tool_name: &str,
    args_json: &str,
    mcp: &McpManager,
) -> (String, bool) {
    match tool_name {
        LIST_TOOL => execute_list(args_json, mcp).await,
        READ_TOOL => execute_read(args_json, mcp).await,
        _ => (format!("Unknown resource tool: {tool_name}"), true),
    }
}

async fn execute_list(args_json: &str, mcp: &McpManager) -> (String, bool) {
    let args: ListArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return (format!("Invalid arguments: {e}"), true),
    };

    let resources = if let Some(ref server_id) = args.server_id {
        match mcp.list_resources(server_id).await {
            Ok(r) => vec![(server_id.clone(), r)],
            Err(e) => return (format!("Failed to list resources: {e}"), true),
        }
    } else {
        mcp.all_resources().await
    };

    if resources.is_empty() || resources.iter().all(|(_, r)| r.is_empty()) {
        return ("No resources available from any connected MCP server.".to_string(), false);
    }

    let mut output = String::new();
    for (server_id, server_resources) in &resources {
        output.push_str(&format!("## Server: {server_id}\n\n"));
        for resource in server_resources {
            output.push_str(&format!("- **{}**\n", resource.name));
            output.push_str(&format!("  URI: `{}`\n", resource.uri));
            if let Some(ref desc) = resource.description {
                output.push_str(&format!("  Description: {desc}\n"));
            }
            if let Some(ref mime) = resource.mime_type {
                output.push_str(&format!("  Type: {mime}\n"));
            }
            output.push('\n');
        }
    }

    (output, false)
}

async fn execute_read(args_json: &str, mcp: &McpManager) -> (String, bool) {
    let args: ReadArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return (format!("Invalid arguments: {e}"), true),
    };

    match mcp.read_resource(&args.server_id, &args.uri).await {
        Ok(result) => {
            let text = result
                .contents
                .iter()
                .filter_map(|c| match c {
                    rmcp::model::ResourceContents::TextResourceContents { text, .. } => {
                        Some(text.clone())
                    }
                    rmcp::model::ResourceContents::BlobResourceContents { blob, .. } => {
                        Some(format!("[Binary data, {} bytes base64]", blob.len()))
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            (text, false)
        }
        Err(e) => (format!("Failed to read resource: {e}"), true),
    }
}
