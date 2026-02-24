use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::agent_config::store::AgentUpdate;
use crate::server::AppState;

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.agents.read().await;
    let agents = store.list();
    Ok(Json(serde_json::to_value(agents).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    // Validate provider exists
    {
        let providers = state.providers.read().await;
        if providers.get(&body.provider_id).is_none() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let mut store = state.agents.write().await;
    let agent = store
        .create_with_mcp(
            body.name,
            body.provider_id,
            body.model,
            body.system_prompt,
            body.temperature,
            body.max_tokens,
            body.mcp_server_ids,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(&agent).unwrap()),
    ))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.agents.read().await;
    match store.get(&id) {
        Some(a) => Ok(Json(serde_json::to_value(a).unwrap())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateAgentRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Validate provider if being changed
    if let Some(ref pid) = body.provider_id {
        let providers = state.providers.read().await;
        if providers.get(pid).is_none() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let mut store = state.agents.write().await;
    let updates = AgentUpdate {
        name: body.name,
        provider_id: body.provider_id,
        model: body.model,
        system_prompt: body.system_prompt.clone(),
        temperature: body.temperature,
        set_temperature: body.set_temperature.unwrap_or(false),
        max_tokens: body.max_tokens,
        set_max_tokens: body.set_max_tokens.unwrap_or(false),
        mcp_server_ids: body.mcp_server_ids,
        set_mcp_server_ids: body.set_mcp_server_ids.unwrap_or(false),
    };

    match store
        .update(&id, updates)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Some(a) => Ok(Json(serde_json::to_value(&a).unwrap())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut store = state.agents.write().await;
    match store.delete(&id) {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn get_active(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let store = state.agents.read().await;
    Json(serde_json::json!({ "agent_id": store.active_agent_id() }))
}

pub async fn set_active(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetActiveRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut store = state.agents.write().await;

    // Validate agent exists if setting one
    if let Some(ref id) = body.agent_id {
        if store.get(id).is_none() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    store
        .set_active(body.agent_id.clone())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "agent_id": body.agent_id })))
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub provider_id: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub mcp_server_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f32>,
    pub set_temperature: Option<bool>,
    pub max_tokens: Option<u32>,
    pub set_max_tokens: Option<bool>,
    pub mcp_server_ids: Option<Vec<String>>,
    pub set_mcp_server_ids: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SetActiveRequest {
    pub agent_id: Option<String>,
}
