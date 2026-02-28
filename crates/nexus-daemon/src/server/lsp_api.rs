use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use super::AppState;

/// List all detected/configured LSP servers with their status.
pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let configs = state.lsp.configs.read().await;
    let settings = configs.settings();
    Json(serde_json::json!({
        "enabled": settings.enabled,
        "diagnostics_timeout_ms": settings.diagnostics_timeout_ms,
        "servers": settings.servers,
    }))
}

/// Toggle an individual LSP server on/off.
pub async fn toggle(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ToggleLspRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = {
        let mut configs = state.lsp.configs.write().await;
        configs
            .set_enabled(&id, body.enabled)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    match result {
        Some(config) => {
            reload_lsp_manager(&state).await;
            state.event_bus.emit_data(
                &id,
                "lsp_server_toggled",
                serde_json::json!({ "id": &id, "enabled": body.enabled }),
            );
            Ok(Json(serde_json::to_value(&config).unwrap()))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Update global LSP settings.
pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateLspSettingsRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    {
        let mut configs = state.lsp.configs.write().await;
        if let Some(enabled) = body.enabled {
            configs
                .set_global_enabled(enabled)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        if let Some(timeout) = body.diagnostics_timeout_ms {
            configs
                .set_diagnostics_timeout(timeout)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    reload_lsp_manager(&state).await;

    let configs = state.lsp.configs.read().await;
    let settings = configs.settings();
    Ok(Json(serde_json::json!({
        "enabled": settings.enabled,
        "diagnostics_timeout_ms": settings.diagnostics_timeout_ms,
        "servers": settings.servers,
    })))
}

/// Re-scan PATH for LSP binaries.
pub async fn detect(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let detected = nexus_lsp::detect::detect_installed_servers();
    let count = detected.len();

    {
        let mut configs = state.lsp.configs.write().await;
        configs.upsert_detected(detected).ok();
    }

    reload_lsp_manager(&state).await;

    state.event_bus.emit_data(
        "lsp",
        "lsp_detection_complete",
        serde_json::json!({ "detected": count }),
    );

    let configs = state.lsp.configs.read().await;
    let settings = configs.settings();
    Json(serde_json::json!({
        "enabled": settings.enabled,
        "diagnostics_timeout_ms": settings.diagnostics_timeout_ms,
        "servers": settings.servers,
    }))
}

/// Rebuild the LspManager with current enabled configs.
async fn reload_lsp_manager(state: &AppState) {
    let configs = state.lsp.configs.read().await;
    let settings = configs.settings();

    let enabled_configs: Vec<_> = if settings.enabled {
        settings
            .servers
            .iter()
            .filter(|c| c.enabled)
            .cloned()
            .collect()
    } else {
        vec![]
    };
    let timeout = settings.diagnostics_timeout_ms;
    drop(configs);

    let mut manager = state.lsp.manager.write().await;
    manager.reload(enabled_configs, timeout).await;
}

#[derive(Debug, Deserialize)]
pub struct ToggleLspRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLspSettingsRequest {
    pub enabled: Option<bool>,
    pub diagnostics_timeout_ms: Option<u64>,
}
