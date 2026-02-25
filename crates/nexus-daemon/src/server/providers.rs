use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::provider::types::{Provider, ProviderPublic, ProviderType};
use crate::provider::store::ProviderUpdate;
use crate::server::AppState;

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.agents.providers.read().await;
    let public: Vec<ProviderPublic> = store.list().iter().map(ProviderPublic::from).collect();
    Ok(Json(serde_json::to_value(public).unwrap()))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProviderRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let mut store = state.agents.providers.write().await;
    let provider = store
        .create(
            body.name,
            body.provider_type,
            body.endpoint,
            body.api_key,
            body.aws_region,
            body.aws_profile,
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
    let store = state.agents.providers.read().await;
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
    let mut store = state.agents.providers.write().await;
    let updates = ProviderUpdate {
        name: body.name,
        endpoint: body.endpoint,
        api_key: body.api_key,
        aws_region: body.aws_region,
        aws_profile: body.aws_profile,
    };

    match store
        .update(&id, updates)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Some(p) => {
            // Invalidate cached client
            state.agents.factory.invalidate(&id).await;
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
    let mut store = state.agents.providers.write().await;
    match store.delete(&id) {
        Ok(true) => {
            let state = Arc::clone(&state);
            tokio::spawn(async move { state.agents.factory.invalidate(&id).await });
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
        let store = state.agents.providers.read().await;
        store.get(&id).cloned().ok_or(StatusCode::NOT_FOUND)?
    };

    match state.agents.factory.get(&provider).await {
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
                ProviderType::Bedrock => "us.anthropic.claude-3-haiku-20240307-v1:0",
            };

            match client
                .create_message_stream(model, 1, None, None, None, messages, vec![])
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

pub async fn test_inline(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProviderRequest>,
) -> Json<serde_json::Value> {
    let provider = Provider {
        id: String::new(),
        name: body.name,
        provider_type: body.provider_type,
        endpoint: body.endpoint,
        api_key: body.api_key,
        aws_region: body.aws_region,
        aws_profile: body.aws_profile,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let test_model = match provider.provider_type {
        ProviderType::Anthropic => "claude-haiku-4-5-20251001",
        ProviderType::Bedrock => "us.anthropic.claude-3-haiku-20240307-v1:0",
    };

    let client = match state.agents.factory.get(&provider).await {
        Ok(c) => c,
        Err(e) => {
            return Json(serde_json::json!({ "ok": false, "error": e.to_string() }));
        }
    };

    let messages = vec![crate::anthropic::types::Message {
        role: crate::anthropic::types::Role::User,
        content: vec![crate::anthropic::types::ContentBlock::Text {
            text: "Hi".to_string(),
        }],
    }];

    match client
        .create_message_stream(test_model, 1, None, None, None, messages, vec![])
        .await
    {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}

pub async fn list_models(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let provider = {
        let store = state.agents.providers.read().await;
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

            if let Some(profile_name) = provider.aws_profile.as_deref() {
                config_loader = config_loader.profile_name(profile_name);
            }

            let sdk_config = config_loader.load().await;
            let client = aws_sdk_bedrock::Client::new(&sdk_config);

            // Only use inference profiles — they cover all models including
            // newer ones (Claude 4+) that aren't in list_foundation_models.
            let mut models: Vec<serde_json::Value> = Vec::new();
            let mut stream = client
                .list_inference_profiles()
                .type_equals(
                    aws_sdk_bedrock::types::InferenceProfileType::SystemDefined,
                )
                .into_paginator()
                .items()
                .send();

            // Non-LLM model prefixes to filter out (after the region prefix)
            let non_llm = ["stability.", "twelvelabs.", "cohere.embed"];

            // Map provider prefix in profile ID → display group name
            let group_of = |model_part: &str| -> &'static str {
                if model_part.starts_with("anthropic.") {
                    "Anthropic"
                } else if model_part.starts_with("amazon.") {
                    "Amazon"
                } else if model_part.starts_with("meta.") {
                    "Meta"
                } else if model_part.starts_with("mistral.") {
                    "Mistral"
                } else if model_part.starts_with("deepseek.") {
                    "DeepSeek"
                } else if model_part.starts_with("writer.") {
                    "Writer"
                } else if model_part.starts_with("cohere.") {
                    "Cohere"
                } else if model_part.starts_with("ai21.") {
                    "AI21 Labs"
                } else {
                    "Other"
                }
            };

            while let Some(Ok(p)) = stream.next().await {
                if *p.status() != aws_sdk_bedrock::types::InferenceProfileStatus::Active
                {
                    continue;
                }
                let id = p.inference_profile_id();
                // Profile IDs are "{region}.{provider}.{model}" — strip the
                // region prefix to check the model family.
                let model_part = id.split_once('.').map(|(_, rest)| rest).unwrap_or(id);
                if non_llm.iter().any(|prefix| model_part.starts_with(prefix)) {
                    continue;
                }
                models.push(serde_json::json!({
                    "id": id,
                    "name": p.inference_profile_name(),
                    "group": group_of(model_part),
                }));
            }

            // Two-layer stable sort:
            //   1) Group: Anthropic → Amazon → everything else (alphabetical)
            //   2) Within group: by embedded release date desc (newest first),
            //      then undated entries in alphanumeric order
            models.sort_by(|a, b| {
                let id_a = a["id"].as_str().unwrap_or("");
                let id_b = b["id"].as_str().unwrap_or("");
                let name_a = a["name"].as_str().unwrap_or("");
                let name_b = b["name"].as_str().unwrap_or("");
                let group_a = a["group"].as_str().unwrap_or("");
                let group_b = b["group"].as_str().unwrap_or("");

                let group_ord = |g: &str| -> u8 {
                    match g {
                        "Anthropic" => 0,
                        "Amazon" => 1,
                        _ => 2,
                    }
                };

                // Extract YYYYMMDD date embedded in profile ID
                let date_of = |id: &str| -> Option<u32> {
                    id.split(|c: char| !c.is_ascii_digit())
                        .find(|s| s.len() == 8 && s.starts_with("20"))
                        .and_then(|s| s.parse().ok())
                };

                group_ord(group_a)
                    .cmp(&group_ord(group_b))
                    .then_with(|| group_a.cmp(group_b))
                    .then_with(|| match (date_of(id_a), date_of(id_b)) {
                        (Some(da), Some(db)) => db.cmp(&da),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => name_a.cmp(name_b),
                    })
            });

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
    pub aws_profile: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub aws_region: Option<String>,
    pub aws_profile: Option<String>,
}
