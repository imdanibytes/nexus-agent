//! Built-in tools that let the LLM agent manage Nexus configuration:
//! projects, workspaces, agents, providers, and MCP servers.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::RwLock;

use crate::agent_config::AgentService;
use crate::agent_config::store::{AgentUpdate, CreateAgentParams};
use crate::anthropic::types::Tool;
use crate::event_bus::EventBus;
use crate::mcp::store::McpServerUpdate;
use crate::mcp::{ClientHandlerState, McpManager};
use crate::project::{ProjectStore, ProjectUpdate};
use crate::provider::store::{CreateProviderParams, ProviderUpdate};
use crate::provider::{ProviderService, ProviderType};
use crate::server::McpService;
use crate::thread::ThreadService;
use crate::workspace::{WorkspaceStore, WorkspaceUpdate};

// Tool names
const PROJECTS: &str = "nexus_projects";
const WORKSPACES: &str = "nexus_workspaces";
const AGENTS: &str = "nexus_agents";
const PROVIDERS: &str = "nexus_providers";
const MCP_SERVERS: &str = "nexus_mcp_servers";

/// Dependencies needed by control plane tools.
pub struct ControlPlaneDeps {
    pub agents: Arc<AgentService>,
    pub providers: Arc<ProviderService>,
    pub projects: Arc<RwLock<ProjectStore>>,
    pub workspaces: Arc<RwLock<WorkspaceStore>>,
    pub threads: Arc<ThreadService>,
    pub mcp_svc: Arc<McpService>,
    pub event_bus: EventBus,
}

pub fn is_control_plane(name: &str) -> bool {
    matches!(name, PROJECTS | WORKSPACES | AGENTS | PROVIDERS | MCP_SERVERS)
}

pub fn tool_definitions() -> Vec<Tool> {
    vec![
        projects_def(),
        workspaces_def(),
        agents_def(),
        providers_def(),
        mcp_servers_def(),
    ]
}

/// Execute a control plane tool. Returns (content, is_error).
pub async fn execute(
    tool_name: &str,
    args_json: &str,
    conversation_id: &str,
    deps: &ControlPlaneDeps,
) -> (String, bool) {
    match tool_name {
        PROJECTS => exec_projects(args_json, deps).await,
        WORKSPACES => exec_workspaces(args_json, conversation_id, deps).await,
        AGENTS => exec_agents(args_json, conversation_id, deps).await,
        PROVIDERS => exec_providers(args_json, deps).await,
        MCP_SERVERS => exec_mcp_servers(args_json, deps).await,
        _ => (format!("Unknown control plane tool: {tool_name}"), true),
    }
}

// ── Tool definitions ─────────────────────────────────────────────

fn projects_def() -> Tool {
    Tool {
        name: PROJECTS.to_string(),
        description: "Manage Nexus projects (codebase roots with filesystem paths). \
            Use action 'list' to see existing projects, 'create' to add a new one, \
            'update' to rename or change path, 'delete' to remove."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "update", "delete"],
                    "description": "Operation to perform"
                },
                "id": { "type": "string", "description": "Project ID (required for update/delete)" },
                "name": { "type": "string", "description": "Project name (required for create)" },
                "path": { "type": "string", "description": "Absolute filesystem path (required for create)" }
            },
            "required": ["action"]
        }),
    }
}

fn workspaces_def() -> Tool {
    Tool {
        name: WORKSPACES.to_string(),
        description: "Manage Nexus workspaces (logical groupings of projects). \
            Use action 'list', 'create', 'update', 'delete', or 'set_active' \
            to set this conversation's workspace context."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "update", "delete", "set_active"],
                    "description": "Operation to perform"
                },
                "id": { "type": "string", "description": "Workspace ID (required for update/delete/set_active)" },
                "name": { "type": "string", "description": "Workspace name (required for create)" },
                "description": { "type": "string", "description": "Optional description" },
                "project_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Project IDs to include in this workspace"
                }
            },
            "required": ["action"]
        }),
    }
}

fn agents_def() -> Tool {
    Tool {
        name: AGENTS.to_string(),
        description: "Manage Nexus agent configurations (LLM personas with model + provider binding). \
            Use action 'list', 'create', 'update', 'delete', or 'set_active' to set this conversation's agent."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "update", "delete", "set_active"],
                    "description": "Operation to perform"
                },
                "id": { "type": "string", "description": "Agent ID (required for update/delete/set_active)" },
                "name": { "type": "string", "description": "Display name (required for create)" },
                "provider_id": { "type": "string", "description": "Provider ID to use (required for create)" },
                "model": { "type": "string", "description": "Model name e.g. 'claude-sonnet-4-20250514' (required for create)" },
                "system_prompt": { "type": "string", "description": "Custom system prompt override" },
                "temperature": { "type": "number", "description": "Sampling temperature (0.0-1.0)" },
                "max_tokens": { "type": "integer", "description": "Max output tokens per response" },
                "mcp_server_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "MCP server IDs this agent can use (omit = all servers)"
                }
            },
            "required": ["action"]
        }),
    }
}

fn providers_def() -> Tool {
    Tool {
        name: PROVIDERS.to_string(),
        description: "Manage Nexus inference providers (API endpoints for LLM inference). \
            Use action 'list', 'create', 'update', or 'delete'. Supported types: 'anthropic', 'bedrock'."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "update", "delete"],
                    "description": "Operation to perform"
                },
                "id": { "type": "string", "description": "Provider ID (required for update/delete)" },
                "name": { "type": "string", "description": "Display name (required for create)" },
                "type": {
                    "type": "string",
                    "enum": ["anthropic", "bedrock"],
                    "description": "Provider type (required for create)"
                },
                "endpoint": { "type": "string", "description": "Custom API base URL (optional)" },
                "api_key": { "type": "string", "description": "API key (for anthropic type)" },
                "aws_region": { "type": "string", "description": "AWS region (for bedrock type)" },
                "aws_profile": { "type": "string", "description": "AWS profile name (for bedrock type)" }
            },
            "required": ["action"]
        }),
    }
}

fn mcp_servers_def() -> Tool {
    Tool {
        name: MCP_SERVERS.to_string(),
        description: "Manage MCP server connections (external tool and resource providers). \
            Use action 'list', 'create', 'update', or 'delete'. Servers can be stdio-based \
            (command + args) or HTTP-based (url)."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "update", "delete"],
                    "description": "Operation to perform"
                },
                "id": { "type": "string", "description": "Server ID (required for update/delete)" },
                "name": { "type": "string", "description": "Display name (required for create)" },
                "command": { "type": "string", "description": "Executable command (stdio mode)" },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Command arguments (stdio mode)"
                },
                "env": {
                    "type": "object",
                    "description": "Environment variables as key-value pairs (stdio mode)"
                },
                "url": { "type": "string", "description": "HTTP endpoint URL (HTTP mode)" },
                "headers": {
                    "type": "object",
                    "description": "Custom HTTP headers as key-value pairs (HTTP mode)"
                }
            },
            "required": ["action"]
        }),
    }
}

// ── Execution ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ActionArgs {
    action: String,
    #[serde(flatten)]
    rest: serde_json::Value,
}

fn get_str(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn require_str(v: &serde_json::Value, key: &str) -> Result<String, (String, bool)> {
    get_str(v, key).ok_or_else(|| (format!("Missing required field: '{key}'"), true))
}

fn format_json(v: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".to_string())
}

// ── Projects ─────────────────────────────────────────────────────

async fn exec_projects(args_json: &str, deps: &ControlPlaneDeps) -> (String, bool) {
    let args: ActionArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return (format!("Invalid arguments: {e}"), true),
    };

    match args.action.as_str() {
        "list" => {
            let store = deps.projects.read().await;
            let projects = store.list();
            (format_json(&projects), false)
        }
        "create" => {
            let name = match require_str(&args.rest, "name") {
                Ok(v) => v,
                Err(e) => return e,
            };
            let path = match require_str(&args.rest, "path") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let mut store = deps.projects.write().await;
            match store.create(name, path) {
                Ok(project) => {
                    deps.event_bus.emit_data(
                        &project.id,
                        "project_created",
                        serde_json::json!({ "id": &project.id, "name": &project.name }),
                    );
                    (format_json(&project), false)
                }
                Err(e) => (format!("Failed to create project: {e}"), true),
            }
        }
        "update" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let updates = ProjectUpdate {
                name: get_str(&args.rest, "name"),
                path: get_str(&args.rest, "path"),
            };

            let mut store = deps.projects.write().await;
            match store.update(&id, updates) {
                Ok(Some(proj)) => {
                    deps.event_bus.emit_data(
                        &proj.id,
                        "project_updated",
                        serde_json::json!({ "id": &proj.id, "name": &proj.name }),
                    );
                    (format_json(&proj), false)
                }
                Ok(None) => (format!("Project not found: {id}"), true),
                Err(e) => (format!("Failed to update project: {e}"), true),
            }
        }
        "delete" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let mut store = deps.projects.write().await;
            match store.delete(&id) {
                Ok(true) => {
                    deps.event_bus.emit_data(
                        &id,
                        "project_deleted",
                        serde_json::json!({ "id": &id }),
                    );
                    (format!("Deleted project {id}"), false)
                }
                Ok(false) => (format!("Project not found: {id}"), true),
                Err(e) => (format!("Failed to delete project: {e}"), true),
            }
        }
        other => (format!("Unknown action for nexus_projects: '{other}'"), true),
    }
}

// ── Workspaces ───────────────────────────────────────────────────

async fn exec_workspaces(args_json: &str, conversation_id: &str, deps: &ControlPlaneDeps) -> (String, bool) {
    let args: ActionArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return (format!("Invalid arguments: {e}"), true),
    };

    match args.action.as_str() {
        "list" => {
            let workspaces = {
                let store = deps.workspaces.read().await;
                serde_json::to_value(store.list()).unwrap_or_default()
            };
            // Show which workspace is set on this conversation
            let conv_workspace_id = deps
                .threads
                .get(conversation_id)
                .await
                .ok()
                .flatten()
                .and_then(|c| c.workspace_id);
            (
                serde_json::json!({
                    "workspaces": workspaces,
                    "current_workspace_id": conv_workspace_id,
                })
                .to_string(),
                false,
            )
        }
        "create" => {
            let name = match require_str(&args.rest, "name") {
                Ok(v) => v,
                Err(e) => return e,
            };
            let description = get_str(&args.rest, "description");
            let project_ids: Vec<String> = args
                .rest
                .get("project_ids")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut store = deps.workspaces.write().await;
            match store.create(name, description, project_ids) {
                Ok(ws) => {
                    deps.event_bus.emit_data(
                        &ws.id,
                        "workspace_created",
                        serde_json::json!({ "id": &ws.id, "name": &ws.name }),
                    );
                    (format_json(&ws), false)
                }
                Err(e) => (format!("Failed to create workspace: {e}"), true),
            }
        }
        "update" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let updates = WorkspaceUpdate {
                name: get_str(&args.rest, "name"),
                description: get_str(&args.rest, "description"),
                project_ids: args
                    .rest
                    .get("project_ids")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
            };

            let mut store = deps.workspaces.write().await;
            match store.update(&id, updates) {
                Ok(Some(ws)) => {
                    deps.event_bus.emit_data(
                        &ws.id,
                        "workspace_updated",
                        serde_json::json!({ "id": &ws.id, "name": &ws.name }),
                    );
                    (format_json(&ws), false)
                }
                Ok(None) => (format!("Workspace not found: {id}"), true),
                Err(e) => (format!("Failed to update workspace: {e}"), true),
            }
        }
        "delete" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let mut store = deps.workspaces.write().await;
            match store.delete(&id) {
                Ok(true) => {
                    deps.event_bus.emit_data(
                        &id,
                        "workspace_deleted",
                        serde_json::json!({ "id": &id }),
                    );
                    (format!("Deleted workspace {id}"), false)
                }
                Ok(false) => (format!("Workspace not found: {id}"), true),
                Err(e) => (format!("Failed to delete workspace: {e}"), true),
            }
        }
        "set_active" => {
            let id = get_str(&args.rest, "id"); // None = clear

            // Validate workspace exists (if setting, not clearing)
            if let Some(ref ws_id) = id {
                let store = deps.workspaces.read().await;
                if store.get(ws_id).is_none() {
                    return (format!("Workspace not found: {ws_id}"), true);
                }
            }

            // Set workspace on the current conversation
            if let Err(e) = deps.threads.set_workspace(conversation_id, id.clone()).await {
                return (format!("Failed to set workspace: {e}"), true);
            }

            match id {
                Some(id) => (format!("Workspace set to {id}"), false),
                None => ("Workspace cleared".to_string(), false),
            }
        }
        other => (format!("Unknown action for nexus_workspaces: '{other}'"), true),
    }
}

// ── Agents ───────────────────────────────────────────────────────

async fn exec_agents(args_json: &str, conversation_id: &str, deps: &ControlPlaneDeps) -> (String, bool) {
    let args: ActionArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return (format!("Invalid arguments: {e}"), true),
    };

    match args.action.as_str() {
        "list" => {
            let agents = deps.agents.list().await;
            let conv_agent_id = deps
                .threads
                .get(conversation_id)
                .await
                .ok()
                .flatten()
                .and_then(|c| c.agent_id);
            let default_agent_id = deps.agents.active_agent_id().await;
            (
                serde_json::json!({
                    "agents": agents,
                    "current_agent_id": conv_agent_id,
                    "default_agent_id": default_agent_id,
                })
                .to_string(),
                false,
            )
        }
        "create" => {
            let name = match require_str(&args.rest, "name") {
                Ok(v) => v,
                Err(e) => return e,
            };
            let provider_id = match require_str(&args.rest, "provider_id") {
                Ok(v) => v,
                Err(e) => return e,
            };
            let model = match require_str(&args.rest, "model") {
                Ok(v) => v,
                Err(e) => return e,
            };

            // Verify provider exists
            if !deps.providers.exists(&provider_id).await {
                return (format!("Provider not found: {provider_id}"), true);
            }

            let params = CreateAgentParams {
                name,
                provider_id,
                model,
                system_prompt: get_str(&args.rest, "system_prompt"),
                temperature: args.rest.get("temperature").and_then(|v| v.as_f64()).map(|f| f as f32),
                max_tokens: args.rest.get("max_tokens").and_then(|v| v.as_u64()).map(|n| n as u32),
                mcp_server_ids: args
                    .rest
                    .get("mcp_server_ids")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
            };

            match deps.agents.create(params).await {
                Ok(agent) => (format_json(&agent), false),
                Err(e) => (format!("Failed to create agent: {e}"), true),
            }
        }
        "update" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let updates = AgentUpdate {
                name: get_str(&args.rest, "name"),
                provider_id: get_str(&args.rest, "provider_id"),
                model: get_str(&args.rest, "model"),
                system_prompt: get_str(&args.rest, "system_prompt"),
                temperature: args.rest.get("temperature").and_then(|v| v.as_f64()).map(|f| f as f32),
                set_temperature: args.rest.get("temperature").is_some(),
                max_tokens: args.rest.get("max_tokens").and_then(|v| v.as_u64()).map(|n| n as u32),
                set_max_tokens: args.rest.get("max_tokens").is_some(),
                thinking_budget: args.rest.get("thinking_budget").and_then(|v| v.as_u64()).map(|n| n as u32),
                set_thinking_budget: args.rest.get("thinking_budget").is_some(),
                mcp_server_ids: args
                    .rest
                    .get("mcp_server_ids")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                set_mcp_server_ids: args.rest.get("mcp_server_ids").is_some(),
            };

            match deps.agents.update(&id, updates).await {
                Ok(Some(agent)) => (format_json(&agent), false),
                Ok(None) => (format!("Agent not found: {id}"), true),
                Err(e) => (format!("Failed to update agent: {e}"), true),
            }
        }
        "delete" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            match deps.agents.delete(&id).await {
                Ok(true) => (format!("Deleted agent {id}"), false),
                Ok(false) => (format!("Agent not found: {id}"), true),
                Err(e) => (format!("Failed to delete agent: {e}"), true),
            }
        }
        "set_active" => {
            let id = get_str(&args.rest, "id");

            // Validate agent exists
            if let Some(ref agent_id) = id {
                if deps.agents.get(agent_id).await.is_none() {
                    return (format!("Agent not found: {agent_id}"), true);
                }
            }

            // Set agent on the current conversation
            if let Err(e) = deps.threads.set_agent(conversation_id, id.clone()).await {
                return (format!("Failed to set agent: {e}"), true);
            }

            // Also update the global default for new conversations
            let _ = deps.agents.set_active(id.clone()).await;

            match id {
                Some(id) => (format!("Agent set to {id}"), false),
                None => ("Agent cleared (will use default)".to_string(), false),
            }
        }
        other => (format!("Unknown action for nexus_agents: '{other}'"), true),
    }
}

// ── Providers ────────────────────────────────────────────────────

async fn exec_providers(args_json: &str, deps: &ControlPlaneDeps) -> (String, bool) {
    let args: ActionArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return (format!("Invalid arguments: {e}"), true),
    };

    match args.action.as_str() {
        "list" => {
            let providers = deps.providers.list().await;
            // Redact api_key for safety
            let safe: Vec<serde_json::Value> = providers
                .iter()
                .map(|p| {
                    let mut v = serde_json::to_value(p).unwrap_or_default();
                    if let Some(obj) = v.as_object_mut() {
                        if obj.contains_key("api_key") {
                            obj.insert("api_key".to_string(), serde_json::json!("***"));
                        }
                    }
                    v
                })
                .collect();
            (format_json(&safe), false)
        }
        "create" => {
            let name = match require_str(&args.rest, "name") {
                Ok(v) => v,
                Err(e) => return e,
            };
            let type_str = match require_str(&args.rest, "type") {
                Ok(v) => v,
                Err(e) => return e,
            };
            let provider_type = match type_str.as_str() {
                "anthropic" => ProviderType::Anthropic,
                "bedrock" => ProviderType::Bedrock,
                other => return (format!("Unknown provider type: '{other}'. Use 'anthropic' or 'bedrock'."), true),
            };

            let params = CreateProviderParams {
                name,
                provider_type,
                endpoint: get_str(&args.rest, "endpoint"),
                api_key: get_str(&args.rest, "api_key"),
                aws_region: get_str(&args.rest, "aws_region"),
                aws_profile: get_str(&args.rest, "aws_profile"),
            };

            match deps.providers.create(params).await {
                Ok(provider) => {
                    let mut v = serde_json::to_value(&provider).unwrap_or_default();
                    if let Some(obj) = v.as_object_mut() {
                        if obj.contains_key("api_key") {
                            obj.insert("api_key".to_string(), serde_json::json!("***"));
                        }
                    }
                    (format_json(&v), false)
                }
                Err(e) => (format!("Failed to create provider: {e}"), true),
            }
        }
        "update" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let updates = ProviderUpdate {
                name: get_str(&args.rest, "name"),
                endpoint: get_str(&args.rest, "endpoint"),
                api_key: get_str(&args.rest, "api_key"),
                aws_region: get_str(&args.rest, "aws_region"),
                aws_profile: get_str(&args.rest, "aws_profile"),
            };

            match deps.providers.update(&id, updates).await {
                Ok(Some(provider)) => {
                    let mut v = serde_json::to_value(&provider).unwrap_or_default();
                    if let Some(obj) = v.as_object_mut() {
                        if obj.contains_key("api_key") {
                            obj.insert("api_key".to_string(), serde_json::json!("***"));
                        }
                    }
                    (format_json(&v), false)
                }
                Ok(None) => (format!("Provider not found: {id}"), true),
                Err(e) => (format!("Failed to update provider: {e}"), true),
            }
        }
        "delete" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            match deps.providers.delete(&id).await {
                Ok(true) => (format!("Deleted provider {id}"), false),
                Ok(false) => (format!("Provider not found: {id}"), true),
                Err(e) => (format!("Failed to delete provider: {e}"), true),
            }
        }
        other => (format!("Unknown action for nexus_providers: '{other}'"), true),
    }
}

// ── MCP Servers ──────────────────────────────────────────────────

async fn exec_mcp_servers(args_json: &str, deps: &ControlPlaneDeps) -> (String, bool) {
    let args: ActionArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return (format!("Invalid arguments: {e}"), true),
    };

    match args.action.as_str() {
        "list" => {
            let store = deps.mcp_svc.configs.read().await;
            let configs = store.list();
            (format_json(&configs), false)
        }
        "create" => {
            let name = match require_str(&args.rest, "name") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let command = get_str(&args.rest, "command").unwrap_or_default();
            let args_vec: Vec<String> = args
                .rest
                .get("args")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let env: HashMap<String, String> = args
                .rest
                .get("env")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let url = get_str(&args.rest, "url");
            let headers: Option<HashMap<String, String>> = args
                .rest
                .get("headers")
                .and_then(|v| serde_json::from_value(v.clone()).ok());

            let config = {
                let mut store = deps.mcp_svc.configs.write().await;
                match store.create(name, command, args_vec, env, url, headers) {
                    Ok(c) => c,
                    Err(e) => return (format!("Failed to create MCP server: {e}"), true),
                }
            };

            reload_mcp(deps).await;
            (format_json(&config), false)
        }
        "update" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let updates = McpServerUpdate {
                name: get_str(&args.rest, "name"),
                command: get_str(&args.rest, "command"),
                args: args
                    .rest
                    .get("args")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                env: args
                    .rest
                    .get("env")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                set_url: args.rest.get("url").is_some(),
                url: get_str(&args.rest, "url"),
                set_headers: args.rest.get("headers").is_some(),
                headers: args
                    .rest
                    .get("headers")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
            };

            let result = {
                let mut store = deps.mcp_svc.configs.write().await;
                store.update(&id, updates)
            };

            match result {
                Ok(Some(config)) => {
                    reload_mcp(deps).await;
                    (format_json(&config), false)
                }
                Ok(None) => (format!("MCP server not found: {id}"), true),
                Err(e) => (format!("Failed to update MCP server: {e}"), true),
            }
        }
        "delete" => {
            let id = match require_str(&args.rest, "id") {
                Ok(v) => v,
                Err(e) => return e,
            };

            let deleted = {
                let mut store = deps.mcp_svc.configs.write().await;
                store.delete(&id)
            };

            match deleted {
                Ok(true) => {
                    reload_mcp(deps).await;
                    (format!("Deleted MCP server {id}"), false)
                }
                Ok(false) => (format!("MCP server not found: {id}"), true),
                Err(e) => (format!("Failed to delete MCP server: {e}"), true),
            }
        }
        other => (format!("Unknown action for nexus_mcp_servers: '{other}'"), true),
    }
}

/// Reload MCP manager after config changes.
async fn reload_mcp(deps: &ControlPlaneDeps) {
    let configs = {
        let store = deps.mcp_svc.configs.read().await;
        store.list().to_vec()
    };

    // Build a minimal ClientHandlerState — we only need the projects lock
    // for the sampling handler. The ControlPlaneDeps doesn't carry it directly,
    // but we can construct one from the projects Arc.
    let handler_state = ClientHandlerState {
        projects: Arc::clone(&deps.projects),
    };
    let new_manager = McpManager::from_configs(&configs, &handler_state).await;

    let mut mcp = deps.mcp_svc.mcp.write().await;
    mcp.shutdown().await;
    *mcp = new_manager;

    tracing::info!(servers = configs.len(), "MCP servers reloaded via control plane tool");
}
