use std::time::Duration;

use serde_json::json;

use crate::fixtures::{self, setup_mock_agent};
use crate::harness::TestDaemon;
use crate::mock_llm::{self, MockLlmServer, MockResponse};

// ── LSP API endpoint tests ──────────────────────────────────────

#[tokio::test]
async fn list_lsp_servers_returns_settings() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/lsp-servers").await;
    assert_eq!(status.as_u16(), 200);
    assert!(body.get("enabled").is_some(), "missing enabled field: {body}");
    assert!(body.get("diagnostics_timeout_ms").is_some(), "missing timeout: {body}");
    assert!(body["servers"].is_array(), "servers should be array: {body}");
}

#[tokio::test]
async fn update_global_lsp_settings() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    // Disable globally
    let (status, body) = c.patch("/api/lsp-settings", &json!({ "enabled": false })).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(body["enabled"], false);

    // Re-enable
    let (status, body) = c.patch("/api/lsp-settings", &json!({ "enabled": true })).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn update_diagnostics_timeout() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.patch("/api/lsp-settings", &json!({ "diagnostics_timeout_ms": 5000 })).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(body["diagnostics_timeout_ms"], 5000);
}

#[tokio::test]
async fn detect_lsp_servers_endpoint() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.post_empty("/api/lsp-servers/detect").await;
    assert_eq!(status.as_u16(), 200);
    assert!(body["servers"].is_array());
}

#[tokio::test]
async fn toggle_nonexistent_server_returns_404() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _) = c.patch("/api/lsp-servers/nonexistent", &json!({ "enabled": false })).await;
    assert_eq!(status.as_u16(), 404);
}

#[tokio::test]
async fn toggle_lsp_server() {
    let d = spawn_daemon_with_mock_lsp().await;
    let c = d.client();

    // List to get the server ID
    let (_, body) = c.get("/api/lsp-servers").await;
    let servers = body["servers"].as_array().expect("servers array");
    assert!(!servers.is_empty(), "should have mock LSP server");
    let server_id = servers[0]["id"].as_str().unwrap();

    // Disable
    let (status, body) = c.patch(
        &format!("/api/lsp-servers/{server_id}"),
        &json!({ "enabled": false }),
    ).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(body["enabled"], false);

    // Re-enable
    let (status, body) = c.patch(
        &format!("/api/lsp-servers/{server_id}"),
        &json!({ "enabled": true }),
    ).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(body["enabled"], true);
}

// ── Decorator integration tests ─────────────────────────────────

#[tokio::test]
async fn read_file_includes_lsp_diagnostics() {
    let d = spawn_daemon_with_mock_lsp().await;
    let c = d.client();

    // Create a project pointing to a temp dir with a Rust file
    let project_dir = d.home_path.join("test-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join("main.rs"), "fn main() {}\n").unwrap();

    let (status, body) = c.post("/api/projects", &fixtures::project_body(
        "test-project",
        project_dir.to_str().unwrap(),
    )).await;
    assert_eq!(status.as_u16(), 201, "create project: {body}");

    // Set up mock agent that calls read_text_file
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "read_text_file",
            "toolu_lsp_001",
            &format!(
                r#"{{"description":"Read file","path":"{}"}}"#,
                project_dir.join("main.rs").display()
            ),
        )),
        MockResponse::Sse(mock_llm::text_response("Read complete")),
    ]).await;

    let (_, _, conv_id) = setup_mock_agent(&c, &mock.url).await;

    let mut sse = d.sse();
    sse.expect_sync().await;

    // Start the turn
    c.post("/api/chat", &json!({
        "conversationId": conv_id,
        "message": "Read the file"
    })).await;

    // Wait for tool result
    let tool_result = sse.expect_event_type("TOOL_CALL_RESULT", Duration::from_secs(15)).await;
    let content = tool_result["content"].as_str().unwrap_or("");

    // Diagnostics are now injected as a separate user message, NOT inside the
    // tool result content. Verify the tool result is clean file content.
    assert!(
        !content.contains("<diagnostics>"),
        "Tool result should NOT contain inline diagnostics (they're a separate message now). Got: {content}"
    );
    assert!(
        content.contains("fn main()"),
        "Tool result should contain the file content. Got: {content}"
    );

    // Wait for turn to finish
    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10)).await;
}

#[tokio::test]
async fn write_file_includes_lsp_diagnostics() {
    let d = spawn_daemon_with_mock_lsp().await;
    let c = d.client();

    let project_dir = d.home_path.join("test-project-write");
    std::fs::create_dir_all(&project_dir).unwrap();

    let (status, body) = c.post("/api/projects", &fixtures::project_body(
        "write-project",
        project_dir.to_str().unwrap(),
    )).await;
    assert_eq!(status.as_u16(), 201, "create project: {body}");

    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "write_file",
            "toolu_lsp_002",
            &format!(
                r#"{{"description":"Write file","path":"{}","content":"fn main() {{ bad_code }}\n"}}"#,
                project_dir.join("main.rs").display()
            ),
        )),
        MockResponse::Sse(mock_llm::text_response("Write complete")),
    ]).await;

    let (_, _, conv_id) = setup_mock_agent(&c, &mock.url).await;

    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post("/api/chat", &json!({
        "conversationId": conv_id,
        "message": "Write the file"
    })).await;

    let tool_result = sse.expect_event_type("TOOL_CALL_RESULT", Duration::from_secs(15)).await;
    let content = tool_result["content"].as_str().unwrap_or("");

    // Diagnostics are injected as a separate user message, not in the tool result.
    assert!(
        !content.contains("<diagnostics>"),
        "Write tool result should NOT contain inline diagnostics. Got: {content}"
    );
    assert!(
        content.contains("Successfully wrote"),
        "Write tool result should confirm the write. Got: {content}"
    );

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10)).await;
}

#[tokio::test]
async fn file_outside_project_gets_no_diagnostics() {
    let d = spawn_daemon_with_mock_lsp().await;
    let c = d.client();

    // Create a project, but read a file OUTSIDE of it
    let project_dir = d.home_path.join("project-scoped");
    std::fs::create_dir_all(&project_dir).unwrap();

    let outside_dir = d.home_path.join("outside");
    std::fs::create_dir_all(&outside_dir).unwrap();
    std::fs::write(outside_dir.join("main.rs"), "fn main() {}\n").unwrap();

    let (status, _) = c.post("/api/projects", &fixtures::project_body(
        "scoped-project",
        project_dir.to_str().unwrap(),
    )).await;
    assert_eq!(status.as_u16(), 201);

    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "read_text_file",
            "toolu_lsp_003",
            &format!(
                r#"{{"description":"Read outside file","path":"{}"}}"#,
                outside_dir.join("main.rs").display()
            ),
        )),
        MockResponse::Sse(mock_llm::text_response("Done")),
    ]).await;

    let (_, _, conv_id) = setup_mock_agent(&c, &mock.url).await;

    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post("/api/chat", &json!({
        "conversationId": conv_id,
        "message": "Read outside file"
    })).await;

    let tool_result = sse.expect_event_type("TOOL_CALL_RESULT", Duration::from_secs(15)).await;
    let content = tool_result["content"].as_str().unwrap_or("");

    assert!(
        !content.contains("<diagnostics>"),
        "File outside project should NOT get diagnostics. Got: {content}"
    );

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10)).await;
}

#[tokio::test]
async fn lsp_disabled_globally_skips_diagnostics() {
    let d = spawn_daemon_with_mock_lsp().await;
    let c = d.client();

    // Disable LSP globally
    c.patch("/api/lsp-settings", &json!({ "enabled": false })).await;

    let project_dir = d.home_path.join("disabled-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join("main.rs"), "fn main() {}\n").unwrap();

    let (status, _) = c.post("/api/projects", &fixtures::project_body(
        "disabled-project",
        project_dir.to_str().unwrap(),
    )).await;
    assert_eq!(status.as_u16(), 201);

    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "read_text_file",
            "toolu_lsp_004",
            &format!(
                r#"{{"description":"Read file","path":"{}"}}"#,
                project_dir.join("main.rs").display()
            ),
        )),
        MockResponse::Sse(mock_llm::text_response("Done")),
    ]).await;

    let (_, _, conv_id) = setup_mock_agent(&c, &mock.url).await;

    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post("/api/chat", &json!({
        "conversationId": conv_id,
        "message": "Read file"
    })).await;

    let tool_result = sse.expect_event_type("TOOL_CALL_RESULT", Duration::from_secs(15)).await;
    let content = tool_result["content"].as_str().unwrap_or("");

    assert!(
        !content.contains("<diagnostics>"),
        "Globally disabled LSP should NOT produce diagnostics. Got: {content}"
    );

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10)).await;
}

// ── Helpers ─────────────────────────────────────────────────────

/// Find the mock-lsp binary path (built by cargo alongside the test binary).
fn mock_lsp_binary() -> String {
    // The binary is in the same target directory as the test binary
    let test_exe = std::env::current_exe().expect("current_exe");
    let target_dir = test_exe.parent().unwrap().parent().unwrap();
    let mock_path = target_dir.join("mock-lsp");
    assert!(
        mock_path.exists(),
        "mock-lsp binary not found at {}. Run `cargo build --bin mock-lsp` first.",
        mock_path.display()
    );
    mock_path.to_str().unwrap().to_string()
}

/// Spawn a daemon with a pre-configured mock LSP server in lsp.json.
/// Uses spawn_at_path so we can write lsp.json BEFORE the daemon starts.
async fn spawn_daemon_with_mock_lsp() -> TestDaemon {
    let mock_path = mock_lsp_binary();

    let home_dir = tempfile::TempDir::new().unwrap();
    let home_path = home_dir.path().to_path_buf();
    let nexus_dir = home_path.join(".nexus");
    std::fs::create_dir_all(&nexus_dir).unwrap();

    // Write required config files
    let config = json!({ "server": { "host": "127.0.0.1", "port": 0 } });
    std::fs::write(
        nexus_dir.join("nexus.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    ).unwrap();
    std::fs::write(nexus_dir.join("mcp.json"), "[]").unwrap();

    // Write lsp.json BEFORE daemon starts so it loads on init
    let lsp_settings = json!({
        "enabled": true,
        "diagnostics_timeout_ms": 5000,
        "servers": [
            {
                "id": "mock-rust-analyzer",
                "name": "Mock rust-analyzer",
                "language_ids": ["rust"],
                "command": mock_path,
                "args": [],
                "enabled": true,
                "auto_detected": false
            }
        ]
    });
    std::fs::write(
        nexus_dir.join("lsp.json"),
        serde_json::to_string_pretty(&lsp_settings).unwrap(),
    ).unwrap();

    // Keep the TempDir alive (the daemon process needs the directory)
    let path = home_dir.path().to_path_buf();
    let _keep = home_dir.keep();
    TestDaemon::spawn_at_path(path).await.unwrap()
}
