use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use super::InferenceProvider;
use crate::error::InferenceError;
use crate::types::{ContentBlock, InferenceRequest, InferenceResponse, StopReason, Usage};

/// Ollama provider using the native `/api/chat` endpoint.
/// Supports thinking/reasoning via `think: true` for models like
/// DeepSeek R1, QwQ, and other reasoning models.
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaProvider {
    /// Connect to a local Ollama instance at the default address.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "http://localhost:11434".into(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    /// Convert our Anthropic-style tool schemas to Ollama's tool format.
    /// Ollama uses the same shape as OpenAI function-calling.
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

    /// Convert our Anthropic-style messages to Ollama's native chat format.
    fn convert_messages(messages: &[Value]) -> Vec<Value> {
        let mut out = Vec::new();

        for msg in messages {
            let role = msg["role"].as_str().unwrap_or("user");

            match role {
                "user" => {
                    if let Some(text) = msg["content"].as_str() {
                        out.push(json!({ "role": "user", "content": text }));
                    } else if let Some(blocks) = msg["content"].as_array() {
                        // Tool results → separate tool messages
                        for block in blocks {
                            if block["type"] == "tool_result" {
                                out.push(json!({
                                    "role": "tool",
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
                                        "function": {
                                            "name": block["name"],
                                            "arguments": block["input"],
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

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InferenceProvider for OllamaProvider {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse, InferenceError> {
        let messages = Self::convert_messages(&request.messages);

        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "stream": false,
        });

        // System prompt goes as a top-level field in Ollama's native API
        // (or as the first system message — but top-level is cleaner)
        if let Some(ref system) = request.system {
            // Prepend as a system message
            let mut all_messages = vec![json!({ "role": "system", "content": system })];
            all_messages.extend(messages);
            body["messages"] = Value::Array(all_messages);
        }

        if !request.tools.is_empty() {
            body["tools"] = Value::Array(Self::convert_tools(&request.tools));
        }

        // Enable thinking if requested
        if request.thinking.is_some() {
            body["think"] = json!(true);
        }

        // Options: set num_predict from max_tokens
        body["options"] = json!({
            "num_predict": request.max_tokens,
        });

        debug!(
            model = %request.model,
            messages = body["messages"].as_array().map(|a| a.len()).unwrap_or(0),
            think = request.thinking.is_some(),
            "ollama inference request"
        );

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
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

        let message = &parsed["message"];
        let mut content = Vec::new();

        // Thinking content (returned when think: true is set)
        if let Some(thinking) = message["thinking"].as_str() {
            if !thinking.is_empty() {
                content.push(ContentBlock::Thinking(thinking.to_string()));
            }
        }

        // Text content
        if let Some(text) = message["content"].as_str() {
            if !text.is_empty() {
                content.push(ContentBlock::Text(text.to_string()));
            }
        }

        // Tool calls
        let has_tool_calls = message["tool_calls"].as_array().is_some_and(|a| !a.is_empty());
        if let Some(tool_calls) = message["tool_calls"].as_array() {
            for tc in tool_calls {
                let name = tc["function"]["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let input = tc["function"]["arguments"].clone();
                // Ollama doesn't return tool call IDs — generate one
                let id = format!("ollama_{}", name);
                content.push(ContentBlock::ToolUse { id, name, input });
            }
        }

        // Determine stop reason
        let stop_reason = if has_tool_calls {
            StopReason::ToolUse
        } else if parsed["done_reason"].as_str() == Some("length") {
            StopReason::MaxTokens
        } else {
            StopReason::EndTurn
        };

        // Ollama returns token counts in the response
        let usage = Usage {
            input_tokens: parsed["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
            output_tokens: parsed["eval_count"].as_u64().unwrap_or(0) as u32,
        };

        Ok(InferenceResponse {
            stop_reason,
            content,
            usage,
        })
    }
}
