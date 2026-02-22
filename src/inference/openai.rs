use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use super::InferenceProvider;
use crate::error::InferenceError;
use crate::types::{ContentBlock, InferenceRequest, InferenceResponse, StopReason, Usage};

/// OpenAI-compatible provider. Works with vLLM, LM Studio, OpenRouter,
/// or any server that implements the `/v1/chat/completions` endpoint.
pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl OpenAiProvider {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: None,
        }
    }

    /// Set an API key (required for OpenAI, OpenRouter, etc.).
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    /// Convert our Anthropic-style tool schemas to OpenAI function-calling format.
    fn convert_tools(tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .filter_map(|tool| {
                let name = tool["name"].as_str()?;
                let description = tool.get("description").cloned().unwrap_or(Value::Null);
                let parameters = tool
                    .get("input_schema")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}}));

                Some(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": description,
                        "parameters": parameters,
                    }
                }))
            })
            .collect()
    }

    /// Convert our Anthropic-style messages to OpenAI chat format.
    fn convert_messages(
        system: Option<&str>,
        messages: &[Value],
    ) -> Vec<Value> {
        let mut out = Vec::new();

        if let Some(sys) = system {
            out.push(json!({ "role": "system", "content": sys }));
        }

        for msg in messages {
            let role = msg["role"].as_str().unwrap_or("user");

            match role {
                "user" => {
                    // Could be a plain string or an array of content blocks (tool results)
                    if let Some(text) = msg["content"].as_str() {
                        out.push(json!({ "role": "user", "content": text }));
                    } else if let Some(blocks) = msg["content"].as_array() {
                        // Convert tool_result blocks to OpenAI tool messages
                        for block in blocks {
                            if block["type"] == "tool_result" {
                                out.push(json!({
                                    "role": "tool",
                                    "tool_call_id": block["tool_use_id"],
                                    "content": block["content"],
                                }));
                            }
                        }
                    }
                }
                "assistant" => {
                    if let Some(blocks) = msg["content"].as_array() {
                        let mut text_parts = Vec::new();
                        let mut tool_calls = Vec::new();

                        for block in blocks {
                            match block["type"].as_str() {
                                Some("text") => {
                                    if let Some(t) = block["text"].as_str() {
                                        text_parts.push(t.to_string());
                                    }
                                }
                                Some("tool_use") => {
                                    tool_calls.push(json!({
                                        "id": block["id"],
                                        "type": "function",
                                        "function": {
                                            "name": block["name"],
                                            "arguments": block["input"].to_string(),
                                        }
                                    }));
                                }
                                _ => {}
                            }
                        }

                        let mut assistant_msg =
                            json!({ "role": "assistant", "content": text_parts.join("\n") });
                        if !tool_calls.is_empty() {
                            assistant_msg["tool_calls"] = Value::Array(tool_calls);
                        }
                        out.push(assistant_msg);
                    } else if let Some(text) = msg["content"].as_str() {
                        out.push(json!({ "role": "assistant", "content": text }));
                    }
                }
                _ => {
                    out.push(msg.clone());
                }
            }
        }

        out
    }
}

#[async_trait]
impl InferenceProvider for OpenAiProvider {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse, InferenceError> {
        let messages =
            Self::convert_messages(request.system.as_deref(), &request.messages);

        let mut body = json!({
            "model": request.model,
            "messages": messages,
        });

        if !request.tools.is_empty() {
            body["tools"] = Value::Array(Self::convert_tools(&request.tools));
        }

        debug!(
            model = %request.model,
            messages = messages.len(),
            "ollama inference request"
        );

        let mut req = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("content-type", "application/json");

        if let Some(ref key) = self.api_key {
            req = req.header("authorization", format!("Bearer {key}"));
        }

        let resp = req
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

        let choice = &parsed["choices"][0];

        // Map OpenAI finish_reason to our StopReason
        let stop_reason = match choice["finish_reason"].as_str().unwrap_or("stop") {
            "stop" => StopReason::EndTurn,
            "tool_calls" => StopReason::ToolUse,
            "length" => StopReason::MaxTokens,
            other => {
                debug!(finish_reason = %other, "unknown finish_reason, treating as EndTurn");
                StopReason::EndTurn
            }
        };

        let message = &choice["message"];
        let mut content = Vec::new();

        // Text content
        if let Some(text) = message["content"].as_str() {
            if !text.is_empty() {
                content.push(ContentBlock::Text(text.to_string()));
            }
        }

        // Tool calls
        if let Some(tool_calls) = message["tool_calls"].as_array() {
            for tc in tool_calls {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let name = tc["function"]["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let input: Value =
                    serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));

                content.push(ContentBlock::ToolUse { id, name, input });
            }
        }

        let usage = Usage {
            input_tokens: parsed["usage"]["prompt_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
            output_tokens: parsed["usage"]["completion_tokens"]
                .as_u64()
                .unwrap_or(0) as u32,
        };

        Ok(InferenceResponse {
            stop_reason,
            content,
            usage,
        })
    }
}
