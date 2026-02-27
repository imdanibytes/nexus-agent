use std::fmt;

/// Normalized error kind across all providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // variants constructed as needed when classifying errors
pub enum ProviderErrorKind {
    RateLimit,
    Authentication,
    InvalidRequest,
    Overloaded,
    ServerError,
    ContextLength,
    NetworkError,
    Unknown,
}

/// Structured error from an inference provider.
///
/// Providers create these instead of raw `anyhow!` strings so the agent loop
/// can downcast and surface rich error data to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    pub retryable: bool,
    pub provider: String,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProviderError {}

impl ProviderError {
    /// User-facing title for this error kind.
    #[allow(dead_code)] // will be used by frontend error rendering
    pub fn title(&self) -> &'static str {
        match self.kind {
            ProviderErrorKind::RateLimit => "Rate limit exceeded",
            ProviderErrorKind::Authentication => "Authentication failed",
            ProviderErrorKind::InvalidRequest => "Invalid request",
            ProviderErrorKind::Overloaded => "Service overloaded",
            ProviderErrorKind::ServerError => "Server error",
            ProviderErrorKind::ContextLength => "Context too long",
            ProviderErrorKind::NetworkError => "Connection error",
            ProviderErrorKind::Unknown => "Error",
        }
    }

    /// Parse an Anthropic HTTP error response into a structured ProviderError.
    pub fn from_anthropic_http(status: reqwest::StatusCode, body: &str) -> Self {
        let (kind, retryable) = match status.as_u16() {
            401 | 403 => (ProviderErrorKind::Authentication, false),
            400 => {
                // Check if it's a context length issue
                if body.contains("prompt is too long")
                    || body.contains("too many tokens")
                    || body.contains("context length")
                {
                    (ProviderErrorKind::ContextLength, false)
                } else {
                    (ProviderErrorKind::InvalidRequest, false)
                }
            }
            429 => (ProviderErrorKind::RateLimit, true),
            529 => (ProviderErrorKind::Overloaded, true),
            500 | 502 | 503 => (ProviderErrorKind::ServerError, true),
            _ => (ProviderErrorKind::Unknown, false),
        };

        // Try to extract the human-readable message from the JSON body
        let message = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("HTTP {}: {}", status, body));

        Self {
            kind,
            message,
            status_code: Some(status.as_u16()),
            retryable,
            provider: "anthropic".to_string(),
        }
    }

    /// Parse an Anthropic SSE error event.
    pub fn from_anthropic_stream(error_type: Option<&str>, message: &str) -> Self {
        let (kind, retryable) = match error_type {
            Some("rate_limit_error") => (ProviderErrorKind::RateLimit, true),
            Some("authentication_error") => (ProviderErrorKind::Authentication, false),
            Some("permission_error") => (ProviderErrorKind::Authentication, false),
            Some("invalid_request_error") => (ProviderErrorKind::InvalidRequest, false),
            Some("overloaded_error") => (ProviderErrorKind::Overloaded, true),
            Some("api_error") => (ProviderErrorKind::ServerError, true),
            _ => (ProviderErrorKind::Unknown, false),
        };

        Self {
            kind,
            message: message.to_string(),
            status_code: None,
            retryable,
            provider: "anthropic".to_string(),
        }
    }

    /// Parse an AWS Bedrock error.
    pub fn from_bedrock(error: &str) -> Self {
        let (kind, retryable) = if error.contains("ThrottlingException")
            || error.contains("rate")
            || error.contains("TooManyRequestsException")
        {
            (ProviderErrorKind::RateLimit, true)
        } else if error.contains("AccessDeniedException")
            || error.contains("UnrecognizedClientException")
        {
            (ProviderErrorKind::Authentication, false)
        } else if error.contains("ValidationException") {
            (ProviderErrorKind::InvalidRequest, false)
        } else if error.contains("ModelStreamErrorException")
            || error.contains("ServiceUnavailableException")
            || error.contains("InternalServerException")
        {
            (ProviderErrorKind::ServerError, true)
        } else if error.contains("ModelTimeoutException") {
            (ProviderErrorKind::Overloaded, true)
        } else {
            (ProviderErrorKind::Unknown, false)
        };

        Self {
            kind,
            message: error.to_string(),
            status_code: None,
            retryable,
            provider: "bedrock".to_string(),
        }
    }
}
