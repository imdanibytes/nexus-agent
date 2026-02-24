use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::provider::types::{ProviderPublic, ProviderType};
use crate::provider::store::ProviderUpdate;
use crate::server::AppState;

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.providers.read().await;
    let public: Vec<ProviderPublic> = store.list().iter().map(ProviderPublic::from).collect();
    Ok(Json(serde_json::to_value(public).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProviderRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let mut store = state.providers.write().await;
    let provider = store
        .create(
            body.name,
            body.provider_type,
            body.endpoint,
            body.api_key,
            body.aws_region,
            body.aws_access_key_id,
            body.aws_secret_access_key,
            body.aws_session_token,
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let public = ProviderPublic::from(&provider);
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(public).unwrap()),
    ))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.providers.read().await;
    match store.get(&id) {
        Some(p) => {
            let public = ProviderPublic::from(p);
            Ok(Json(serde_json::to_value(public).unwrap()))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateProviderRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut store = state.providers.write().await;
    let updates = ProviderUpdate {
        name: body.name,
        endpoint: body.endpoint,
        api_key: body.api_key,
        aws_region: body.aws_region,
        aws_access_key_id: body.aws_access_key_id,
        aws_secret_access_key: body.aws_secret_access_key,
        aws_session_token: body.aws_session_token,
    };

    match store
        .update(&id, updates)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Some(p) => {
            // Invalidate cached client
            state.factory.invalidate(&id).await;
            let public = ProviderPublic::from(&p);
            Ok(Json(serde_json::to_value(public).unwrap()))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut store = state.providers.write().await;
    match store.delete(&id) {
        Ok(true) => {
            let state = Arc::clone(&state);
            tokio::spawn(async move { state.factory.invalidate(&id).await });
            StatusCode::NO_CONTENT
        }
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn test_connection(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let provider = {
        let store = state.providers.read().await;
        store.get(&id).cloned().ok_or(StatusCode::NOT_FOUND)?
    };

    match state.factory.get(&provider).await {
        Ok(client) => {
            // Send a minimal request to test the connection
            let messages = vec![crate::anthropic::types::Message {
                role: crate::anthropic::types::Role::User,
                content: vec![crate::anthropic::types::ContentBlock::Text {
                    text: "Hi".to_string(),
                }],
            }];

            let model = match provider.provider_type {
                ProviderType::Anthropic => "claude-haiku-4-5-20251001",
                ProviderType::Bedrock => "anthropic.claude-3-haiku-20240307-v1:0",
            };

            match client
                .create_message_stream(model, 1, None, None, messages, vec![])
                .await
            {
                Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
                Err(e) => Ok(Json(
                    serde_json::json!({ "ok": false, "error": e.to_string() }),
                )),
            }
        }
        Err(e) => Ok(Json(
            serde_json::json!({ "ok": false, "error": e.to_string() }),
        )),
    }
}

pub async fn list_models(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let provider = {
        let store = state.providers.read().await;
        store.get(&id).cloned().ok_or(StatusCode::NOT_FOUND)?
    };

    match provider.provider_type {
        ProviderType::Anthropic => {
            let base_url = provider
                .endpoint
                .as_deref()
                .unwrap_or("https://api.anthropic.com");
            let api_key = provider.api_key.as_deref().unwrap_or_default();

            let client = reqwest::Client::new();
            let resp = client
                .get(format!("{}/v1/models", base_url))
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(|_| StatusCode::BAD_GATEWAY)?;

            if !resp.status().is_success() {
                return Ok(Json(serde_json::json!({ "models": [] })));
            }

            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let models: Vec<serde_json::Value> = body
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|m| {
                            serde_json::json!({
                                "id": m.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                                "name": m.get("display_name")
                                    .and_then(|v| v.as_str())
                                    .or_else(|| m.get("id").and_then(|v| v.as_str()))
                                    .unwrap_or(""),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            Ok(Json(serde_json::json!({ "models": models })))
        }
        ProviderType::Bedrock => {
            let region = provider.aws_region.as_deref().unwrap_or("us-east-1");
            let mut config_loader =
                aws_config::from_env().region(aws_config::Region::new(region.to_string()));

            if let (Some(key), Some(secret)) = (
                provider.aws_access_key_id.as_deref(),
                provider.aws_secret_access_key.as_deref(),
            ) {
                let creds = aws_sdk_bedrockruntime::config::Credentials::new(
                    key,
                    secret,
                    provider.aws_session_token.clone(),
                    None,
                    "nexus",
                );
                config_loader = config_loader.credentials_provider(creds);
            }

            let sdk_config = config_loader.load().await;
            let client = aws_sdk_bedrock::Client::new(&sdk_config);

            let resp = client
                .list_foundation_models()
                .by_inference_type(aws_sdk_bedrock::types::InferenceType::OnDemand)
                .send()
                .await
                .map_err(|_| StatusCode::BAD_GATEWAY)?;

            let models: Vec<serde_json::Value> = resp
                .model_summaries()
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "id": m.model_id(),
                        "name": m.model_name().unwrap_or(m.model_id()),
                    })
                })
                .collect();

            Ok(Json(serde_json::json!({ "models": models })))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub aws_region: Option<String>,
    pub aws_access_key_id: Option<String>,
    pub aws_secret_access_key: Option<String>,
    pub aws_session_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub aws_region: Option<String>,
    pub aws_access_key_id: Option<String>,
    pub aws_secret_access_key: Option<String>,
    pub aws_session_token: Option<String>,
}
