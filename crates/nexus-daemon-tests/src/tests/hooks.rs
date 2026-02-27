//! Integration tests for the DaemonModule hook system.
//!
//! These tests exercise the debug-only HookProbe module, which records hook
//! invocations and supports configurable behavior (deny tools, force continuation).
//! The probe is automatically registered in debug builds and exposed via
//! `/api/debug/hooks*` endpoints.

use std::time::Duration;

use serde_json::json;

use crate::fixtures::setup_mock_agent;
use crate::harness::TestDaemon;
use crate::mock_llm::{self, MockLlmServer, MockResponse};

/// Helper: start a turn and return the run response.
async fn start_turn(
    client: &crate::client::DaemonClient,
    conversation_id: &str,
    message: &str,
) -> serde_json::Value {
    let (status, body) = client
        .post(
            "/api/chat",
            &json!({
                "conversationId": conversation_id,
                "message": message
            }),
        )
        .await;
    assert_eq!(status.as_u16(), 200, "start_turn failed: {body}");
    body
}

/// Helper: get all hook records from the probe.
async fn get_hook_records(client: &crate::client::DaemonClient) -> Vec<serde_json::Value> {
    let (status, body) = client.get("/api/debug/hooks").await;
    assert_eq!(status.as_u16(), 200, "get hooks failed: {body}");
    body.as_array()
        .expect("hooks response should be array")
        .clone()
}

/// Helper: extract hook names from records, optionally filtered by conversation.
fn hook_names(records: &[serde_json::Value], conversation_id: Option<&str>) -> Vec<String> {
    records
        .iter()
        .filter(|r| {
            conversation_id
                .map(|id| r["conversation_id"].as_str() == Some(id))
                .unwrap_or(true)
        })
        .map(|r| r["hook"].as_str().unwrap_or("").to_string())
        .collect()
}

/// Poll hook records until `expected_hook` appears for the given conversation,
/// or timeout. `turn_end` fires after RUN_FINISHED so we need to poll.
async fn wait_for_hook(
    client: &crate::client::DaemonClient,
    conversation_id: &str,
    expected_hook: &str,
    timeout: Duration,
) -> Vec<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let records = get_hook_records(client).await;
        let names = hook_names(&records, Some(conversation_id));
        if names.contains(&expected_hook.to_string()) {
            return records;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Timed out waiting for hook '{expected_hook}' for conversation {conversation_id}. Got: {names:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ── Test 1: Hook lifecycle fires in order for text response ──

#[tokio::test]
async fn hook_lifecycle_fires_in_order_for_text_response() {
    let mock = MockLlmServer::start(vec![MockResponse::Sse(mock_llm::text_response(
        "Hello from hooks test!",
    ))])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    // Clear any startup records
    client.post_empty("/api/debug/hooks/clear").await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Hi").await;

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // turn_end fires after RUN_FINISHED — poll until it appears
    let records = wait_for_hook(&client, &conv_id, "turn_end", Duration::from_secs(5)).await;
    let names = hook_names(&records, Some(&conv_id));

    // Expected order: user_prompt_submit → turn_start → stop → turn_end
    assert!(
        names.contains(&"user_prompt_submit".to_string()),
        "Missing user_prompt_submit in: {names:?}"
    );
    assert!(
        names.contains(&"turn_start".to_string()),
        "Missing turn_start in: {names:?}"
    );
    assert!(
        names.contains(&"stop".to_string()),
        "Missing stop in: {names:?}"
    );
    assert!(
        names.contains(&"turn_end".to_string()),
        "Missing turn_end in: {names:?}"
    );

    // Verify ordering: each hook should appear after the previous one
    let prompt_idx = names.iter().position(|n| n == "user_prompt_submit").unwrap();
    let start_idx = names.iter().position(|n| n == "turn_start").unwrap();
    let stop_idx = names.iter().position(|n| n == "stop").unwrap();
    let end_idx = names.iter().position(|n| n == "turn_end").unwrap();

    assert!(
        prompt_idx < start_idx,
        "user_prompt_submit ({prompt_idx}) should come before turn_start ({start_idx})"
    );
    assert!(
        start_idx < stop_idx,
        "turn_start ({start_idx}) should come before stop ({stop_idx})"
    );
    assert!(
        stop_idx < end_idx,
        "stop ({stop_idx}) should come before turn_end ({end_idx})"
    );
}

// ── Test 2: Hook lifecycle fires for tool use ──

#[tokio::test]
async fn hook_lifecycle_fires_for_tool_use() {
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "nexus_read_file",
            "toolu_hook_001",
            r#"{"description":"Reading test file","path":"/tmp/nexus-hook-test.txt"}"#,
        )),
        MockResponse::Sse(mock_llm::text_response("Got the file")),
    ])
    .await;

    std::fs::write("/tmp/nexus-hook-test.txt", "hook test content").ok();

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    client.post_empty("/api/debug/hooks/clear").await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Read a file").await;

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // turn_end fires after RUN_FINISHED — poll until it appears
    let records = wait_for_hook(&client, &conv_id, "turn_end", Duration::from_secs(5)).await;
    let names = hook_names(&records, Some(&conv_id));

    // Should contain tool lifecycle hooks
    assert!(
        names.contains(&"turn_start".to_string()),
        "Missing turn_start in: {names:?}"
    );
    assert!(
        names.contains(&"pre_tool_use".to_string()),
        "Missing pre_tool_use in: {names:?}"
    );

    // Tool may succeed (post_tool_use) or fail validation (post_tool_use_failure)
    // depending on filesystem config. Either is fine — we're testing the hook fires.
    let has_post = names.contains(&"post_tool_use".to_string())
        || names.contains(&"post_tool_use_failure".to_string());
    assert!(
        has_post,
        "Missing post_tool_use or post_tool_use_failure in: {names:?}"
    );

    assert!(
        names.contains(&"stop".to_string()),
        "Missing stop in: {names:?}"
    );
    assert!(
        names.contains(&"turn_end".to_string()),
        "Missing turn_end in: {names:?}"
    );

    // Verify tool hook ordering: pre_tool_use → (post_tool_use | post_tool_use_failure) → stop
    let pre_idx = names.iter().position(|n| n == "pre_tool_use").unwrap();
    let post_idx = names
        .iter()
        .position(|n| n == "post_tool_use" || n == "post_tool_use_failure")
        .unwrap();
    let stop_idx = names.iter().position(|n| n == "stop").unwrap();

    assert!(
        pre_idx < post_idx,
        "pre_tool_use ({pre_idx}) should come before post hook ({post_idx})"
    );
    assert!(
        post_idx < stop_idx,
        "post hook ({post_idx}) should come before stop ({stop_idx})"
    );

    // Verify the pre_tool_use record contains the tool name
    let pre_record = records
        .iter()
        .find(|r| {
            r["hook"].as_str() == Some("pre_tool_use")
                && r["conversation_id"].as_str() == Some(&conv_id)
        })
        .expect("pre_tool_use record not found");
    assert_eq!(
        pre_record["details"]["tool_name"].as_str(),
        Some("nexus_read_file"),
    );

    std::fs::remove_file("/tmp/nexus-hook-test.txt").ok();
}

// ── Test 3: pre_tool_use deny blocks execution ──

#[tokio::test]
async fn pre_tool_use_deny_blocks_execution() {
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "nexus_read_file",
            "toolu_deny_001",
            r#"{"description":"Reading blocked file","path":"/tmp/nexus-deny-test.txt"}"#,
        )),
        MockResponse::Sse(mock_llm::text_response("Tool was denied")),
    ])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    client.post_empty("/api/debug/hooks/clear").await;

    // Deny nexus_read_file before the turn
    client
        .post(
            "/api/debug/hooks/deny-tool",
            &json!({ "tool_name": "nexus_read_file" }),
        )
        .await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Read a file").await;

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    let records = get_hook_records(&client).await;
    let names = hook_names(&records, Some(&conv_id));

    // pre_tool_use should fire with denied=true
    let pre_record = records
        .iter()
        .find(|r| {
            r["hook"].as_str() == Some("pre_tool_use")
                && r["conversation_id"].as_str() == Some(&conv_id)
        })
        .expect("pre_tool_use record not found");
    assert_eq!(pre_record["details"]["denied"], json!(true));

    // Neither post_tool_use nor post_tool_use_failure should fire for the denied tool
    assert!(
        !names.contains(&"post_tool_use".to_string()),
        "post_tool_use should NOT fire for denied tool, got: {names:?}"
    );
    assert!(
        !names.contains(&"post_tool_use_failure".to_string()),
        "post_tool_use_failure should NOT fire for denied tool, got: {names:?}"
    );
}

// ── Test 4: stop hook can force continuation ──

#[tokio::test]
async fn stop_hook_can_force_continuation() {
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::text_response("First response")),
        MockResponse::Sse(mock_llm::text_response("Second response after continuation")),
    ])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    client.post_empty("/api/debug/hooks/clear").await;

    // Force one continuation — the first stop will return Continue, the second will Stop
    client
        .post(
            "/api/debug/hooks/force-continue",
            &json!({ "count": 1 }),
        )
        .await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Test continuation").await;

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(15))
        .await;

    // Poll for turn_end to ensure all hooks have fired
    let records = wait_for_hook(&client, &conv_id, "turn_end", Duration::from_secs(5)).await;

    // Should have TWO stop records for this conversation
    let stop_records: Vec<&serde_json::Value> = records
        .iter()
        .filter(|r| {
            r["hook"].as_str() == Some("stop")
                && r["conversation_id"].as_str() == Some(conv_id.as_str())
        })
        .collect();

    assert_eq!(
        stop_records.len(),
        2,
        "Expected 2 stop records, got {}: {stop_records:?}",
        stop_records.len()
    );

    // First stop should be Continue, second should be Stop
    assert_eq!(
        stop_records[0]["details"]["decision"].as_str(),
        Some("Continue"),
        "First stop should be Continue: {:?}",
        stop_records[0]
    );
    assert_eq!(
        stop_records[1]["details"]["decision"].as_str(),
        Some("Stop"),
        "Second stop should be Stop: {:?}",
        stop_records[1]
    );
}

// ── Test 5: post_tool_use_failure fires on tool error ──

#[tokio::test]
async fn post_tool_use_failure_fires_on_tool_error() {
    // Use a non-existent tool name — tool dispatch will return an error
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "nonexistent_tool_xyz",
            "toolu_fail_001",
            r#"{"description":"This tool does not exist"}"#,
        )),
        MockResponse::Sse(mock_llm::text_response("Handled the error")),
    ])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    client.post_empty("/api/debug/hooks/clear").await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Use unknown tool").await;

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // Poll for turn_end to ensure all hooks have fired
    let records = wait_for_hook(&client, &conv_id, "turn_end", Duration::from_secs(5)).await;
    let names = hook_names(&records, Some(&conv_id));

    // post_tool_use_failure should fire for the unknown tool
    assert!(
        names.contains(&"post_tool_use_failure".to_string()),
        "Missing post_tool_use_failure in: {names:?}"
    );

    let failure_record = records
        .iter()
        .find(|r| {
            r["hook"].as_str() == Some("post_tool_use_failure")
                && r["conversation_id"].as_str() == Some(&conv_id)
        })
        .expect("post_tool_use_failure record not found");

    assert_eq!(
        failure_record["details"]["tool_name"].as_str(),
        Some("nonexistent_tool_xyz"),
    );
}

// ── Test 6: hooks clear endpoint resets state ──

#[tokio::test]
async fn hooks_clear_endpoint_resets_state() {
    let mock = MockLlmServer::start(vec![MockResponse::Sse(mock_llm::text_response(
        "Generate some records",
    ))])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Hello").await;

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // Wait for turn_end to fire (it comes after RUN_FINISHED)
    wait_for_hook(&client, &conv_id, "turn_end", Duration::from_secs(5)).await;

    // Should have records now
    let records = get_hook_records(&client).await;
    assert!(
        !records.is_empty(),
        "Should have records after a turn"
    );

    // Clear
    let (status, _) = client.post_empty("/api/debug/hooks/clear").await;
    assert_eq!(status.as_u16(), 200);

    // Should be empty
    let records = get_hook_records(&client).await;
    assert!(
        records.is_empty(),
        "Records should be empty after clear, got: {records:?}"
    );
}
