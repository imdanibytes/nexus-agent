use serde_json::{json, Value};

pub fn provider_body(name: &str) -> Value {
    json!({
        "name": name,
        "type": "anthropic",
        "api_key": "test-key-not-real"
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
