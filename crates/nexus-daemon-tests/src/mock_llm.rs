//! Mock Anthropic Messages API server for deterministic integration tests.
//!
//! Spins up a lightweight axum server that handles `POST /v1/messages` and
//! returns pre-configured SSE responses. The daemon's `AnthropicProvider`
//! already supports custom endpoints — we just point it at this server.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Response, StatusCode};
use axum::routing::post;
use axum::Router;
use std::collections::VecDeque;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

/// A mock server that mimics the Anthropic Messages API.
pub struct MockLlmServer {
    pub url: String,
    pub port: u16,
    /// Captured request bodies (for asserting what the daemon sent).
    pub requests: Arc<Mutex<Vec<serde_json::Value>>>,
    _task: JoinHandle<()>,
}

/// A pre-configured response for the mock server.
pub enum MockResponse {
    /// Return a canned SSE stream (200 OK).
    Sse(String),
    /// Return an HTTP error.
    Error { status: u16, body: String },
    /// Delay before returning SSE (for abort/timeout tests).
    Delayed { delay_ms: u64, sse: String },
}

#[derive(Clone)]
struct MockState {
    responses: Arc<Mutex<VecDeque<MockResponse>>>,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl MockLlmServer {
    /// Start a mock server with a queue of responses.
    /// Each `POST /v1/messages` pops the next response from the queue.
    pub async fn start(responses: Vec<MockResponse>) -> Self {
        // Allocate a free port (bind + release), then rebind with tokio
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind for port");
            listener.local_addr().unwrap().port()
        };
        let url = format!("http://127.0.0.1:{port}");

        let state = MockState {
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
            requests: Arc::new(Mutex::new(Vec::new())),
        };
        let requests = state.requests.clone();

        let app = Router::new()
            .route("/v1/messages", post(handle_messages))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .expect("bind mock server");

        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        Self {
            url,
            port,
            requests,
            _task: task,
        }
    }

    /// Get all captured request bodies.
    pub fn captured_requests(&self) -> Vec<serde_json::Value> {
        self.requests.lock().unwrap().clone()
    }
}

async fn handle_messages(
    State(state): State<MockState>,
    body: String,
) -> Response<Body> {
    // Capture the request body
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        state.requests.lock().unwrap().push(json);
    }

    let response = state
        .responses
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or_else(|| {
            MockResponse::Error {
                status: 500,
                body: r#"{"error":{"type":"test_error","message":"No more mock responses queued"}}"#
                    .to_string(),
            }
        });

    match response {
        MockResponse::Sse(sse_body) => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .body(Body::from(sse_body))
            .unwrap(),
        MockResponse::Error { status, body } => Response::builder()
            .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap(),
        MockResponse::Delayed { delay_ms, sse } => {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/event-stream")
                .body(Body::from(sse))
                .unwrap()
        }
    }
}

// ── Response builders ──────────────────────────────────────────────

/// Build an SSE response for a simple text reply.
pub fn text_response(text: &str) -> String {
    text_response_with_usage(text, 100, 25)
}

/// Build an SSE response for a text reply with explicit usage.
pub fn text_response_with_usage(text: &str, input_tokens: u32, output_tokens: u32) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "event: message_start\n\
         data: {{\"message\":{{\"id\":\"msg_mock_001\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"mock-model\",\"usage\":{{\"input_tokens\":{input_tokens},\"output_tokens\":0}}}}}}\n\n\
         event: content_block_start\n\
         data: {{\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n\
         event: content_block_delta\n\
         data: {{\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{escaped}\"}}}}\n\n\
         event: content_block_stop\n\
         data: {{\"index\":0}}\n\n\
         event: message_delta\n\
         data: {{\"delta\":{{\"stop_reason\":\"end_turn\"}},\"usage\":{{\"output_tokens\":{output_tokens}}}}}\n\n\
         event: message_stop\n\
         data: {{}}\n\n"
    )
}

/// Build an SSE response for a tool use call.
pub fn tool_use_response(tool_name: &str, tool_id: &str, args_json: &str) -> String {
    let escaped_args = args_json.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "event: message_start\n\
         data: {{\"message\":{{\"id\":\"msg_mock_tool\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"mock-model\",\"usage\":{{\"input_tokens\":50,\"output_tokens\":0}}}}}}\n\n\
         event: content_block_start\n\
         data: {{\"index\":0,\"content_block\":{{\"type\":\"tool_use\",\"id\":\"{tool_id}\",\"name\":\"{tool_name}\"}}}}\n\n\
         event: content_block_delta\n\
         data: {{\"index\":0,\"delta\":{{\"type\":\"input_json_delta\",\"partial_json\":\"{escaped_args}\"}}}}\n\n\
         event: content_block_stop\n\
         data: {{\"index\":0}}\n\n\
         event: message_delta\n\
         data: {{\"delta\":{{\"stop_reason\":\"tool_use\"}},\"usage\":{{\"output_tokens\":30}}}}\n\n\
         event: message_stop\n\
         data: {{}}\n\n"
    )
}

/// Build an SSE error event.
pub fn error_response(error_type: &str, message: &str) -> MockResponse {
    MockResponse::Error {
        status: 500,
        body: format!(
            "{{\"type\":\"error\",\"error\":{{\"type\":\"{error_type}\",\"message\":\"{message}\"}}}}"
        ),
    }
}
