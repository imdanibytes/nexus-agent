use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;
use crate::workspace::WorkspaceUpdate;

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.workspaces.read().await;
    let workspaces = store.list();
    Ok(Json(serde_json::to_value(workspaces).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace = {
        let mut store = state.workspaces.write().await;
        store
            .create(body.name, body.path)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    // Refresh effective filesystem config
    reload_effective_config(&state).await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(&workspace).unwrap()),
    ))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkspaceRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = {
        let mut store = state.workspaces.write().await;
        let updates = WorkspaceUpdate {
            name: body.name,
            path: body.path,
        };
        store
            .update(&id, updates)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    match result {
        Some(ws) => {
            reload_effective_config(&state).await;
            Ok(Json(serde_json::to_value(&ws).unwrap()))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let deleted = {
        let mut store = state.workspaces.write().await;
        store.delete(&id)
    };

    match deleted {
        Ok(true) => {
            reload_effective_config(&state).await;
            StatusCode::NO_CONTENT
        }
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Recompute effective filesystem config from current workspaces + config.
async fn reload_effective_config(state: &Arc<AppState>) {
    let store = state.workspaces.read().await;
    let workspace_paths: Vec<String> = store.list().iter().map(|w| w.path.clone()).collect();
    drop(store);

    let mut dirs = Vec::new();
    for path in &workspace_paths {
        if !dirs.contains(path) {
            dirs.push(path.clone());
        }
    }
    for dir in &state.base_filesystem_config.allowed_directories {
        if !dirs.contains(dir) {
            dirs.push(dir.clone());
        }
    }

    let effective = crate::config::FilesystemConfig {
        enabled: state.base_filesystem_config.enabled,
        allowed_directories: dirs,
    };

    let mut fs_config = state.effective_fs_config.write().await;
    *fs_config = effective;

    tracing::debug!(
        workspace_count = workspace_paths.len(),
        "Effective filesystem config updated"
    );
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub path: Option<String>,
}
