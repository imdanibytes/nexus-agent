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
    let threads = state.threads.list().await;
    Ok(Json(serde_json::to_value(&threads).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    body: Option<Json<CreateRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client_id = body.and_then(|b| b.id.clone());
    let meta = state
        .threads
        .create(client_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(&meta).unwrap()))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conv = state
        .threads
        .get(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut value = serde_json::to_value(&conv).unwrap();
    // Include task state (triggers lazy disk load via get_or_default)
    let task_state = state.tasks.get_or_default(&id).await;
    if task_state.plan.is_some() {
        value["task_state"] = serde_json::to_value(&task_state).unwrap();
    }
    Ok(Json(value))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    // Cancel running background processes and clean up output files
    state.chat.process_manager.cleanup_conversation(&id).await;

    match state.threads.delete(&id).await {
        Ok(()) => {
            // Clean up associated task state (memory + disk)
            state.tasks.remove(&id).await;
            StatusCode::NO_CONTENT
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn rename(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> StatusCode {
    match state.threads.rename(&id, &body.title).await {
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
    let mut conv = state
        .threads
        .checkout(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Cannot switch to a path inside a sealed span
    if conv.is_in_sealed_span(&body.message_id) {
        return Err(StatusCode::CONFLICT);
    }

    let new_path = conv.path_to(&body.message_id);
    if new_path.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    conv.active_path = new_path;
    conv.updated_at = Utc::now();
    let mut value = serde_json::to_value(&conv).unwrap();
    state
        .threads
        .commit(conv)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Include task state if it exists
    let task_state = state.tasks.get_or_default(&id).await;
    if task_state.plan.is_some() {
        value["task_state"] = serde_json::to_value(&task_state).unwrap();
    }
    Ok(Json(value))
}
