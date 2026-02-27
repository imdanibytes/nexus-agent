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

pub async fn get_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.workspaces.read().await;
    match store.get(&id) {
        Some(ws) => Ok(Json(serde_json::to_value(ws).unwrap())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace = {
        let mut store = state.workspaces.write().await;
        store
            .create(body.name, body.description, body.project_ids.unwrap_or_default())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    state.event_bus.emit_data(
        &workspace.id,
        "workspace_created",
        serde_json::json!({ "id": &workspace.id, "name": &workspace.name }),
    );

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
            description: body.description,
            project_ids: body.project_ids,
        };
        store
            .update(&id, updates)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    match result {
        Some(ws) => {
            state.event_bus.emit_data(
                &ws.id,
                "workspace_updated",
                serde_json::json!({ "id": &ws.id, "name": &ws.name }),
            );
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
            state.event_bus.emit_data(
                &id,
                "workspace_deleted",
                serde_json::json!({ "id": &id }),
            );
            StatusCode::NO_CONTENT
        }
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}


#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub description: Option<String>,
    pub project_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub project_ids: Option<Vec<String>>,
}

