use anyhow::{anyhow, Result};
use async_trait::async_trait;
use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::Client as BedrockClient;
use futures::stream::BoxStream;
use futures::StreamExt;

use crate::anthropic::types::{Message, StreamEvent, Tool};
use crate::provider::InferenceProvider;

pub struct BedrockProvider {
    client: BedrockClient,
}

impl BedrockProvider {
    pub async fn new(region: &str, profile: Option<&str>) -> Result<Self> {
        let mut config_loader =
            aws_config::from_env().region(aws_config::Region::new(region.to_string()));

        if let Some(profile_name) = profile {
            config_loader = config_loader.profile_name(profile_name);
        }

        let config = config_loader.load().await;
        let client = BedrockClient::new(&config);

        Ok(Self { client })
    }
}

#[async_trait]
impl InferenceProvider for BedrockProvider {
    async fn create_message_stream(
        &self,
        model: &str,
        max_tokens: u32,
        system: Option<String>,
        temperature: Option<f32>,
        messages: Vec<Message>,
        tools: Vec<Tool>,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        // Build the Anthropic Messages API request body for invoke_model
        let mut body = serde_json::json!({
            "anthropic_version": "bedrock-2023-05-31",
            "max_tokens": max_tokens,
            "messages": messages,
        });

        if let Some(sys) = system {
            body["system"] = serde_json::Value::String(sys);
        }
        if let Some(t) = temperature {
            body["temperature"] = serde_json::json!(t);
        }
        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(&tools)?;
        }

        tracing::debug!(
            model = model,
            tools = tools.len(),
            body = %serde_json::to_string_pretty(&body).unwrap_or_default(),
            "Bedrock request body"
        );

        let body_bytes = serde_json::to_vec(&body)?;

        let output = self
            .client
            .invoke_model_with_response_stream()
            .model_id(model)
            .content_type("application/json")
            .body(Blob::new(body_bytes))
            .send()
            .await
            .map_err(|e| anyhow!("Bedrock invoke error: {:?}", e))?;

        let event_stream = output.body;

        // Each chunk from Bedrock contains a JSON payload that matches Anthropic's SSE events
        let stream = futures::stream::unfold(event_stream, |mut receiver| async move {
            loop {
                match receiver.recv().await {
                    Ok(Some(event)) => {
                        use aws_sdk_bedrockruntime::types::ResponseStream;
                        match event {
                            ResponseStream::Chunk(payload_part) => {
                                if let Some(bytes) = payload_part.bytes() {
                                    match parse_bedrock_chunk(bytes.as_ref()) {
                                        Ok(Some(stream_event)) => {
                                            return Some((Ok(stream_event), receiver));
                                        }
                                        Ok(None) => continue,
                                        Err(e) => return Some((Err(e), receiver)),
                                    }
                                }
                                continue;
                            }
                            _ => continue,
                        }
                    }
                    Ok(None) => return None,
                    Err(e) => {
                        return Some((
                            Err(anyhow!("Bedrock stream error: {}", e)),
                            receiver,
                        ));
                    }
                }
            }
        });

        Ok(stream.boxed())
    }
}

/// Parse a Bedrock EventStream chunk payload into our StreamEvent.
/// Bedrock wraps Anthropic-format JSON events in its EventStream frames.
fn parse_bedrock_chunk(bytes: &[u8]) -> Result<Option<StreamEvent>> {
    let json: serde_json::Value = serde_json::from_slice(bytes)?;

    let event_type = json["type"].as_str().unwrap_or("");

    match event_type {
        "message_start" => {
            let msg = &json["message"];
            Ok(Some(StreamEvent::MessageStart {
                message_id: msg["id"].as_str().unwrap_or("").to_string(),
                model: msg["model"].as_str().unwrap_or("").to_string(),
                role: serde_json::from_value(msg["role"].clone())
                    .unwrap_or(crate::anthropic::types::Role::Assistant),
                usage: msg.get("usage").and_then(|u| serde_json::from_value(u.clone()).ok()),
            }))
        }
        "content_block_start" => {
            let index = json["index"].as_u64().unwrap_or(0) as usize;
            let cb = &json["content_block"];
            let cb_type = cb["type"].as_str().unwrap_or("");
            let info = match cb_type {
                "text" => crate::anthropic::types::ContentBlockInfo::Text,
                "tool_use" => crate::anthropic::types::ContentBlockInfo::ToolUse {
                    id: cb["id"].as_str().unwrap_or("").to_string(),
                    name: cb["name"].as_str().unwrap_or("").to_string(),
                },
                "thinking" => crate::anthropic::types::ContentBlockInfo::Thinking,
                _ => return Ok(None),
            };
            Ok(Some(StreamEvent::ContentBlockStart {
                index,
                content_block: info,
            }))
        }
        "content_block_delta" => {
            let index = json["index"].as_u64().unwrap_or(0) as usize;
            let delta = &json["delta"];
            let delta_type = delta["type"].as_str().unwrap_or("");
            let d = match delta_type {
                "text_delta" => crate::anthropic::types::Delta::TextDelta {
                    text: delta["text"].as_str().unwrap_or("").to_string(),
                },
                "input_json_delta" => crate::anthropic::types::Delta::InputJsonDelta {
                    partial_json: delta["partial_json"].as_str().unwrap_or("").to_string(),
                },
                "thinking_delta" => crate::anthropic::types::Delta::ThinkingDelta {
                    thinking: delta["thinking"].as_str().unwrap_or("").to_string(),
                },
                _ => return Ok(None),
            };
            Ok(Some(StreamEvent::ContentBlockDelta { index, delta: d }))
        }
        "content_block_stop" => {
            let index = json["index"].as_u64().unwrap_or(0) as usize;
            Ok(Some(StreamEvent::ContentBlockStop { index }))
        }
        "message_delta" => {
            let delta = &json["delta"];
            let stop_reason = delta
                .get("stop_reason")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let usage = json
                .get("usage")
                .and_then(|u| serde_json::from_value(u.clone()).ok());
            Ok(Some(StreamEvent::MessageDelta {
                stop_reason,
                usage,
            }))
        }
        "message_stop" => Ok(Some(StreamEvent::MessageStop)),
        "ping" => Ok(Some(StreamEvent::Ping)),
        "error" => {
            let msg = json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();
            Ok(Some(StreamEvent::Error { message: msg }))
        }
        _ => Ok(None),
    }
}
