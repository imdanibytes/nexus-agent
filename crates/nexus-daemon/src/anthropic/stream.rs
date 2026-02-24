use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::types::*;

/// Parses a raw SSE byte stream from the Anthropic API into typed StreamEvents.
pub struct SseStream<S> {
    inner: S,
    buffer: String,
}

impl<S> SseStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
        }
    }
}

impl<S> Stream for SseStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<StreamEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        loop {
            // Try to extract a complete SSE event from the buffer
            if let Some(event) = try_parse_event(&mut this.buffer)? {
                return Poll::Ready(Some(Ok(event)));
            }

            // Need more data
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    this.buffer.push_str(&String::from_utf8_lossy(&bytes));
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(anyhow!("Stream error: {}", e))));
                }
                Poll::Ready(None) => {
                    // Stream ended. If there's remaining data, try one more parse
                    if !this.buffer.trim().is_empty() {
                        if let Some(event) = try_parse_event(&mut this.buffer)? {
                            return Poll::Ready(Some(Ok(event)));
                        }
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Try to extract one complete SSE event (terminated by \n\n) from the buffer.
/// Returns None if no complete event is available yet.
fn try_parse_event(buffer: &mut String) -> Result<Option<StreamEvent>> {
    // SSE events are separated by blank lines (\n\n)
    let Some(end) = buffer.find("\n\n") else {
        return Ok(None);
    };

    let event_text = buffer[..end].to_string();
    *buffer = buffer[end + 2..].to_string();

    parse_sse_event(&event_text)
}

/// Parse a single SSE event block into a StreamEvent.
fn parse_sse_event(text: &str) -> Result<Option<StreamEvent>> {
    let mut event_type = None;
    let mut data = None;

    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event: ") {
            event_type = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data: ") {
            data = Some(value.to_string());
        } else if line.starts_with("event:") {
            event_type = Some(line["event:".len()..].trim().to_string());
        } else if line.starts_with("data:") {
            data = Some(line["data:".len()..].trim().to_string());
        }
    }

    let Some(event_type) = event_type else {
        return Ok(None);
    };

    let data_str = data.as_deref().unwrap_or("{}");

    let event = match event_type.as_str() {
        "message_start" => {
            let raw: RawMessageStart = serde_json::from_str(data_str)?;
            StreamEvent::MessageStart {
                message_id: raw.message.id,
                model: raw.message.model,
                role: raw.message.role,
                usage: raw.message.usage,
            }
        }
        "content_block_start" => {
            let raw: RawContentBlockStart = serde_json::from_str(data_str)?;
            let info = match raw.content_block {
                RawContentBlock::Text { .. } => ContentBlockInfo::Text,
                RawContentBlock::ToolUse { id, name } => ContentBlockInfo::ToolUse { id, name },
                RawContentBlock::Thinking { .. } => ContentBlockInfo::Thinking,
            };
            StreamEvent::ContentBlockStart {
                index: raw.index,
                content_block: info,
            }
        }
        "content_block_delta" => {
            let raw: RawContentBlockDelta = serde_json::from_str(data_str)?;
            let delta = match raw.delta {
                RawDelta::TextDelta { text } => Delta::TextDelta { text },
                RawDelta::InputJsonDelta { partial_json } => {
                    Delta::InputJsonDelta { partial_json }
                }
                RawDelta::ThinkingDelta { thinking } => Delta::ThinkingDelta { thinking },
            };
            StreamEvent::ContentBlockDelta {
                index: raw.index,
                delta,
            }
        }
        "content_block_stop" => {
            let raw: RawContentBlockStop = serde_json::from_str(data_str)?;
            StreamEvent::ContentBlockStop { index: raw.index }
        }
        "message_delta" => {
            let raw: RawMessageDelta = serde_json::from_str(data_str)?;
            StreamEvent::MessageDelta {
                stop_reason: raw.delta.stop_reason,
                usage: raw.usage,
            }
        }
        "message_stop" => StreamEvent::MessageStop,
        "ping" => StreamEvent::Ping,
        "error" => {
            let raw: RawError = serde_json::from_str(data_str)?;
            StreamEvent::Error {
                error_type: raw.error.error_type,
                message: raw.error.message,
            }
        }
        _ => {
            tracing::debug!("Unknown SSE event type: {}", event_type);
            return Ok(None);
        }
    };

    Ok(Some(event))
}
