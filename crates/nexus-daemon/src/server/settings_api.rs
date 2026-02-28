use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::config::NexusConfig;
use super::AppState;

/// Return general settings (model tiers, etc.).
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "model_tiers": state.config.model_tiers,
    }))
}

/// Update model tier overrides. Persists to nexus.json immediately.
pub async fn update_model_tiers(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<UpdateModelTiersRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Load current config from disk (avoid stale in-memory state)
    let mut config = NexusConfig::load()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load config: {}", e)))?;

    // Merge — only update fields that were sent
    if body.fast.is_some() {
        config.model_tiers.fast = body.fast;
    }
    if body.balanced.is_some() {
        config.model_tiers.balanced = body.balanced;
    }
    if body.smart.is_some() {
        config.model_tiers.smart = body.smart;
    }

    config.save()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {}", e)))?;

    Ok(Json(serde_json::json!({
        "model_tiers": config.model_tiers,
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateModelTiersRequest {
    #[serde(default)]
    pub fast: Option<String>,
    #[serde(default)]
    pub balanced: Option<String>,
    #[serde(default)]
    pub smart: Option<String>,
}
