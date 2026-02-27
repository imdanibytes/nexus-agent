use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::mcp::{ClientHandlerState, McpManager};
use crate::mcp::store::McpServerUpdate;
use crate::server::AppState;

/// Shut down old MCP servers and spawn new ones from the current config.
async fn reload_mcp(state: &Arc<AppState>) {
    let configs = {
        let store = state.mcp.configs.read().await;
        store.list().to_vec()
    };

    let handler_state = ClientHandlerState {
        workspaces: Arc::clone(&state.workspaces),
    };
    let new_manager = McpManager::from_configs(&configs, &handler_state).await;

    let mut mcp = state.mcp.mcp.write().await;
    mcp.shutdown().await;
    *mcp = new_manager;

    tracing::info!(servers = configs.len(), "MCP servers reloaded");
}

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.mcp.configs.read().await;
    let configs = store.list();
    Ok(Json(serde_json::to_value(configs).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateMcpServerRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let config = {
        let mut store = state.mcp.configs.write().await;
        store
            .create(
                body.name,
                body.command.unwrap_or_default(),
                body.args.unwrap_or_default(),
                body.env.unwrap_or_default(),
                body.url,
                body.headers,
            )
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    reload_mcp(&state).await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(&config).unwrap()),
    ))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMcpServerRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = {
        let mut store = state.mcp.configs.write().await;
        let updates = McpServerUpdate {
            name: body.name,
            command: body.command,
            args: body.args,
            env: body.env,
            set_url: body.set_url.unwrap_or(false),
            url: body.url,
            set_headers: body.set_headers.unwrap_or(false),
            headers: body.headers,
        };

        store
            .update(&id, updates)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    match result {
        Some(c) => {
            reload_mcp(&state).await;
            Ok(Json(serde_json::to_value(&c).unwrap()))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let deleted = {
        let mut store = state.mcp.configs.write().await;
        store.delete(&id)
    };

    match deleted {
        Ok(true) => {
            reload_mcp(&state).await;
            StatusCode::NO_CONTENT
        }
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Test an MCP server configuration by spawning it, listing tools, then shutting down.
pub async fn test_inline(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateMcpServerRequest>,
) -> Json<serde_json::Value> {
    use crate::config::McpServerConfig;
    use crate::mcp::server::McpServer;

    let config = McpServerConfig {
        id: "test".to_string(),
        name: body.name.clone(),
        command: body.command.unwrap_or_default(),
        args: body.args.unwrap_or_default(),
        env: body.env.unwrap_or_default(),
        url: body.url,
        headers: body.headers,
    };

    let handler_state = ClientHandlerState {
        workspaces: Arc::clone(&state.workspaces),
    };
    match McpServer::spawn(&config, &handler_state).await {
        Ok(srv) => {
            let tool_names: Vec<String> = srv.tools().iter().map(|t| t.name.to_string()).collect();
            let count = tool_names.len();
            srv.shutdown().await;
            Json(serde_json::json!({
                "ok": true,
                "tools": count,
                "tool_names": tool_names,
            }))
        }
        Err(e) => {
            Json(serde_json::json!({
                "ok": false,
                "error": e.to_string(),
            }))
        }
    }
}

pub async fn list_resources(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp = state.mcp.mcp.read().await;
    match mcp.list_resources(&id).await {
        Ok(resources) => Ok(Json(serde_json::to_value(&resources).unwrap())),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn read_resource(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ReadResourceRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mcp = state.mcp.mcp.read().await;
    match mcp.read_resource(&id, &body.uri).await {
        Ok(result) => Ok(Json(serde_json::to_value(&result).unwrap())),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Deserialize)]
pub struct ReadResourceRequest {
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateMcpServerRequest {
    pub name: String,
    #[serde(default)]
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    /// Set to true to explicitly clear/update the URL field.
    pub set_url: Option<bool>,
    /// Set to true to explicitly clear/update the headers field.
    pub set_headers: Option<bool>,
}
