use async_trait::async_trait;
use serde_json::Value;

use crate::error::InferenceError;
use crate::types::{ContentBlock, InferenceRequest, InferenceResponse, StopReason, Usage};

/// Pure LLM API call. No state, no history, no context management.
/// Request in, response out.
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse, InferenceError>;
}

/// Claude API client via Anthropic's messages endpoint.
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.anthropic.com".into(),
        }
    }

    pub fn with_client(client: reqwest::Client, api_key: impl Into<String>) -> Self {
        Self {
            client,
            api_key: api_key.into(),
            base_url: "https://api.anthropic.com".into(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait]
impl InferenceProvider for AnthropicProvider {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse, InferenceError> {
        let mut body = serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "messages": request.messages,
        });

        if let Some(ref system) = request.system {
            body["system"] = Value::String(system.clone());
        }

        if !request.tools.is_empty() {
            body["tools"] = Value::Array(request.tools);
        }

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| InferenceError::Request(e.to_string()))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| InferenceError::Request(e.to_string()))?;

        if status != 200 {
            return Err(InferenceError::ApiError { status, body: text });
        }

        let parsed: Value =
            serde_json::from_str(&text).map_err(|e| InferenceError::Parse(e.to_string()))?;

        let stop_reason = match parsed["stop_reason"].as_str().unwrap_or("unknown") {
            "end_turn" => StopReason::EndTurn,
            "tool_use" => StopReason::ToolUse,
            "max_tokens" => StopReason::MaxTokens,
            other => {
                return Err(InferenceError::Parse(format!(
                    "unknown stop_reason: {other}"
                )))
            }
        };

        let raw = parsed["content"].as_array().cloned().unwrap_or_default();
        let content = raw
            .iter()
            .filter_map(|block| match block["type"].as_str()? {
                "text" => Some(ContentBlock::Text(
                    block["text"].as_str().unwrap_or("").to_string(),
                )),
                "tool_use" => Some(ContentBlock::ToolUse {
                    id: block["id"].as_str()?.to_string(),
                    name: block["name"].as_str()?.to_string(),
                    input: block["input"].clone(),
                }),
                _ => None,
            })
            .collect();

        let usage = Usage {
            input_tokens: parsed["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: parsed["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        };

        Ok(InferenceResponse {
            stop_reason,
            content,
            usage,
        })
    }
}
