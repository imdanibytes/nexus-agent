use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::mcp::McpManager;
use crate::mcp::store::McpServerUpdate;
use crate::server::AppState;

/// Shut down old MCP servers and spawn new ones from the current config.
async fn reload_mcp(state: &Arc<AppState>) {
    let configs = {
        let store = state.mcp_configs.read().await;
        store.list().to_vec()
    };

    let new_manager = McpManager::from_configs(&configs).await;

    let mut mcp = state.mcp.write().await;
    mcp.shutdown().await;
    *mcp = new_manager;

    tracing::info!(servers = configs.len(), "MCP servers reloaded");
}

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.mcp_configs.read().await;
    let configs = store.list();
    Ok(Json(serde_json::to_value(configs).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateMcpServerRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let config = {
        let mut store = state.mcp_configs.write().await;
        store
            .create(body.name, body.command, body.args.unwrap_or_default(), body.env.unwrap_or_default())
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
        let mut store = state.mcp_configs.write().await;
        let updates = McpServerUpdate {
            name: body.name,
            command: body.command,
            args: body.args,
            env: body.env,
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
        let mut store = state.mcp_configs.write().await;
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
    Json(body): Json<CreateMcpServerRequest>,
) -> Json<serde_json::Value> {
    use crate::config::McpServerConfig;
    use crate::mcp::server::McpServer;

    let config = McpServerConfig {
        id: "test".to_string(),
        name: body.name.clone(),
        command: body.command,
        args: body.args.unwrap_or_default(),
        env: body.env.unwrap_or_default(),
    };

    match McpServer::spawn(&config).await {
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

#[derive(Debug, Deserialize)]
pub struct CreateMcpServerRequest {
    pub name: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
}
