use anyhow::Result;

use super::stream::SseStream;
use super::types::*;
use crate::provider::error::ProviderError;

#[derive(Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            base_url: "https://api.anthropic.com".to_string(),
        }
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            base_url,
        }
    }

    /// Send a non-streaming Messages API request. Returns the full response.
    pub async fn create_message(
        &self,
        request: MessagesRequest,
    ) -> Result<MessagesResponse> {
        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::from_anthropic_http(status, &body).into());
        }

        let response: MessagesResponse = resp.json().await?;
        Ok(response)
    }

    /// Send a streaming Messages API request. Returns a stream of parsed SSE events.
    #[allow(dead_code)] // part of client API, callers use create_message_stream_json
    pub async fn create_message_stream(
        &self,
        request: MessagesRequest,
    ) -> Result<SseStream<impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin>>
    {
        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::from_anthropic_http(status, &body).into());
        }

        Ok(SseStream::new(resp.bytes_stream()))
    }

    /// Send a streaming request with a pre-built JSON body.
    ///
    /// Used when prompt caching injection modifies the serialized request
    /// (e.g., converting system prompt to array format, adding cache_control).
    ///
    /// `extra_headers` allows injecting additional headers (e.g., `anthropic-beta`
    /// for extended thinking).
    pub async fn create_message_stream_json(
        &self,
        body: serde_json::Value,
        extra_headers: Option<Vec<(&str, &str)>>,
    ) -> Result<SseStream<impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin>>
    {
        let mut req = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");

        if let Some(headers) = extra_headers {
            for (key, value) in headers {
                req = req.header(key, value);
            }
        }

        let resp = req.json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::from_anthropic_http(status, &body).into());
        }

        Ok(SseStream::new(resp.bytes_stream()))
    }
}
