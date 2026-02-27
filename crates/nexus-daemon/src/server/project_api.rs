use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::project::ProjectUpdate;
use crate::server::AppState;

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.projects.read().await;
    let projects = store.list();
    Ok(Json(serde_json::to_value(projects).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let project = {
        let mut store = state.projects.write().await;
        store
            .create(body.name, body.path)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    // Refresh effective filesystem config
    reload_effective_config(&state).await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(&project).unwrap()),
    ))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateProjectRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = {
        let mut store = state.projects.write().await;
        let updates = ProjectUpdate {
            name: body.name,
            path: body.path,
        };
        store
            .update(&id, updates)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    match result {
        Some(proj) => {
            reload_effective_config(&state).await;
            Ok(Json(serde_json::to_value(&proj).unwrap()))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let deleted = {
        let mut store = state.projects.write().await;
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

/// Recompute effective filesystem config from current projects + config.
async fn reload_effective_config(state: &Arc<AppState>) {
    let store = state.projects.read().await;
    let project_paths: Vec<String> = store.list().iter().map(|p| p.path.clone()).collect();
    drop(store);

    let mut dirs = Vec::new();
    for path in &project_paths {
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
        project_count = project_paths.len(),
        "Effective filesystem config updated"
    );
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub path: Option<String>,
}
