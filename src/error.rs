#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("inference error: {0}")]
    Inference(#[from] InferenceError),
    #[error("agent cancelled")]
    Cancelled,
    #[error("session error: {0}")]
    Session(String),
    #[error("context error: {0}")]
    Context(String),
}

#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("request failed: {0}")]
    Request(String),
    #[error("API returned {status}: {body}")]
    ApiError { status: u16, body: String },
    #[error("failed to parse response: {0}")]
    Parse(String),
}
