use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;

use crate::server::AppState;

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.conversations.read().await;
    let threads = store.list();
    Ok(Json(serde_json::to_value(threads).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    body: Option<Json<CreateRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client_id = body.and_then(|b| b.id.clone());
    let mut store = state.conversations.write().await;
    let meta = store
        .create(client_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(&meta).unwrap()))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.conversations.read().await;
    match store.get(&id) {
        Ok(Some(conv)) => {
            let branch_info = conv.branch_info();
            let mut val = serde_json::to_value(&conv).unwrap();
            if !branch_info.is_empty() {
                val["branch_info"] = serde_json::to_value(&branch_info).unwrap();
            }
            Ok(Json(val))
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut store = state.conversations.write().await;
    match store.delete(&id) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn rename(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> StatusCode {
    let mut store = state.conversations.write().await;
    match store.rename(&id, &body.title) {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    pub id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct SwitchPathRequest {
    #[serde(rename = "messageId")]
    pub message_id: String,
}

pub async fn switch_path(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SwitchPathRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut store = state.conversations.write().await;
    let mut conv = store
        .get(&id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let new_path = conv.path_to(&body.message_id);
    if new_path.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    conv.active_path = new_path;
    conv.updated_at = Utc::now();
    store
        .save(&conv)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let branch_info = conv.branch_info();
    let mut val = serde_json::to_value(&conv).unwrap();
    if !branch_info.is_empty() {
        val["branch_info"] = serde_json::to_value(&branch_info).unwrap();
    }

    Ok(Json(val))
}
