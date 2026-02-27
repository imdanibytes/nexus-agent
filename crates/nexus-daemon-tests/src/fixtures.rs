use serde_json::{json, Value};

use crate::client::DaemonClient;

pub fn provider_body(name: &str) -> Value {
    json!({
        "name": name,
        "type": "anthropic",
        "api_key": "test-key-not-real"
    })
}

/// Provider body with a custom endpoint (for mock LLM server).
pub fn mock_provider_body(name: &str, endpoint: &str) -> Value {
    json!({
        "name": name,
        "type": "anthropic",
        "api_key": "mock-key",
        "endpoint": endpoint
    })
}

pub fn agent_body(name: &str, provider_id: &str) -> Value {
    json!({
        "name": name,
        "provider_id": provider_id,
        "model": "claude-sonnet-4-20250514"
    })
}

pub fn project_body(name: &str, path: &str) -> Value {
    json!({ "name": name, "path": path })
}

pub fn workspace_body(name: &str) -> Value {
    json!({ "name": name })
}

pub fn mcp_server_body(name: &str) -> Value {
    json!({
        "name": name,
        "command": "echo",
        "args": ["hello"]
    })
}

/// Create provider → agent → set active → create conversation.
/// Returns (provider_id, agent_id, conversation_id).
pub async fn setup_mock_agent(
    client: &DaemonClient,
    mock_url: &str,
) -> (String, String, String) {
    // Create provider pointing to mock
    let (status, body) = client.post("/api/providers", &mock_provider_body("mock-provider", mock_url)).await;
    assert_eq!(status.as_u16(), 201, "create provider: {body}");
    let provider_id = body["id"].as_str().unwrap().to_string();

    // Create agent using that provider
    let (status, body) = client.post("/api/agents", &agent_body("mock-agent", &provider_id)).await;
    assert_eq!(status.as_u16(), 201, "create agent: {body}");
    let agent_id = body["id"].as_str().unwrap().to_string();

    // Set as active
    let (status, _) = client.put("/api/agents/active", &json!({ "agent_id": &agent_id })).await;
    assert_eq!(status.as_u16(), 200, "set active agent");

    // Create conversation
    let (status, body) = client.post("/api/conversations", &json!({})).await;
    assert!(status.is_success(), "create conversation: {body}");
    let conversation_id = body["id"].as_str().unwrap().to_string();

    (provider_id, agent_id, conversation_id)
}
