use std::time::Duration;

use serde_json::json;

use crate::fixtures::setup_mock_agent;
use crate::harness::TestDaemon;
use crate::mock_llm::{self, MockLlmServer, MockResponse};

fn is_type(event: &serde_json::Value, ty: &str) -> bool {
    event.get("type").and_then(|t| t.as_str()) == Some(ty)
}

fn _is_custom(event: &serde_json::Value, name: &str) -> bool {
    is_type(event, "CUSTOM") && event.get("name").and_then(|n| n.as_str()) == Some(name)
}

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

// ── Text response tests ──────────────────────────────────────────

#[tokio::test]
async fn text_response_emits_run_started_and_finished() {
    let mock = MockLlmServer::start(vec![MockResponse::Sse(mock_llm::text_response(
        "Hello from mock!",
    ))])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Hi").await;

    // Should see RUN_STARTED
    let started = sse
        .expect_event_type("RUN_STARTED", Duration::from_secs(10))
        .await;
    assert_eq!(
        started.get("threadId").and_then(|t| t.as_str()),
        Some(conv_id.as_str())
    );
    assert!(started.get("runId").and_then(|r| r.as_str()).is_some());

    // Should see RUN_FINISHED
    let finished = sse
        .expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;
    assert_eq!(
        finished.get("threadId").and_then(|t| t.as_str()),
        Some(conv_id.as_str())
    );
}

#[tokio::test]
async fn text_response_emits_text_message_events() {
    let mock = MockLlmServer::start(vec![MockResponse::Sse(mock_llm::text_response(
        "Test content",
    ))])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Say something").await;

    // TEXT_MESSAGE_START
    let start = sse
        .expect_event_type("TEXT_MESSAGE_START", Duration::from_secs(10))
        .await;
    assert!(start.get("messageId").is_some());

    // TEXT_MESSAGE_CONTENT with delta
    let content = sse
        .expect_event_type("TEXT_MESSAGE_CONTENT", Duration::from_secs(5))
        .await;
    assert_eq!(
        content.get("delta").and_then(|d| d.as_str()),
        Some("Test content")
    );

    // TEXT_MESSAGE_END
    sse.expect_event_type("TEXT_MESSAGE_END", Duration::from_secs(5))
        .await;
}

#[tokio::test]
async fn text_response_persists_messages() {
    let mock = MockLlmServer::start(vec![MockResponse::Sse(mock_llm::text_response(
        "Persisted reply",
    ))])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Remember this").await;

    // Wait for turn to complete
    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // Fetch conversation — should have messages. Retry briefly since
    // persistence may flush slightly after the RUN_FINISHED event.
    let mut messages_len = 0;
    for _ in 0..10 {
        let (status, conv) = client.get(&format!("/api/conversations/{conv_id}")).await;
        assert_eq!(status.as_u16(), 200);
        let messages = conv["messages"].as_array().expect("messages should be array");
        messages_len = messages.len();
        if messages_len >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        messages_len >= 2,
        "Expected at least 2 messages (user + assistant), got {}",
        messages_len
    );
}

#[tokio::test]
async fn usage_update_event_contains_token_counts() {
    let mock = MockLlmServer::start(vec![MockResponse::Sse(
        mock_llm::text_response_with_usage("token test", 150, 42),
    )])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Count tokens").await;

    // Wait for usage_update CUSTOM event
    let usage = sse.expect_custom("usage_update", Duration::from_secs(10)).await;
    let value = &usage["value"];

    // Should have token counts (exact values depend on daemon's usage aggregation)
    assert!(
        value.get("inputTokens").is_some(),
        "usage_update missing inputTokens: {value}"
    );
    assert!(
        value.get("outputTokens").is_some(),
        "usage_update missing outputTokens: {value}"
    );
}

#[tokio::test]
async fn all_turn_events_have_matching_thread_and_run_ids() {
    let mock = MockLlmServer::start(vec![MockResponse::Sse(mock_llm::text_response(
        "ID check",
    ))])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Check IDs").await;

    // Collect all turn events until RUN_FINISHED
    let events = sse
        .collect_matching(
            |e| {
                // Collect turn-scoped events (have runId)
                e.get("runId").is_some()
            },
            Duration::from_secs(10),
        )
        .await;

    assert!(!events.is_empty(), "Should have collected turn events");

    // All should have the same threadId and runId
    let thread_id = events[0]
        .get("threadId")
        .and_then(|t| t.as_str())
        .expect("first event missing threadId");
    let run_id = events[0]
        .get("runId")
        .and_then(|r| r.as_str())
        .expect("first event missing runId");

    assert_eq!(thread_id, conv_id);

    for event in &events {
        assert_eq!(
            event.get("threadId").and_then(|t| t.as_str()),
            Some(thread_id),
            "threadId mismatch in event: {event}"
        );
        assert_eq!(
            event.get("runId").and_then(|r| r.as_str()),
            Some(run_id),
            "runId mismatch in event: {event}"
        );
    }
}

// ── Error handling tests ──────────────────────────────────────────

#[tokio::test]
async fn provider_error_emits_run_error() {
    let mock = MockLlmServer::start(vec![mock_llm::error_response(
        "overloaded_error",
        "Server is overloaded",
    )])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Trigger error").await;

    // Should get RUN_ERROR (possibly after retries)
    let error = sse
        .expect_event_type("RUN_ERROR", Duration::from_secs(30))
        .await;
    assert!(
        error.get("message").is_some(),
        "RUN_ERROR should have message field: {error}"
    );
}

#[tokio::test]
async fn abort_stops_running_turn() {
    // Delayed response gives us time to abort.
    // Use a short delay — the daemon's cancel token drops the reqwest future,
    // but the mock still holds the connection for delay_ms.
    let mock = MockLlmServer::start(vec![MockResponse::Delayed {
        delay_ms: 3_000,
        sse: mock_llm::text_response("Should not see this"),
    }])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Abort me").await;

    // Wait for RUN_STARTED to confirm turn is active
    sse.expect_event_type("RUN_STARTED", Duration::from_secs(5))
        .await;

    // Abort the turn
    let (status, _) = client
        .post("/api/chat/abort", &json!({ "conversationId": &conv_id }))
        .await;
    assert_eq!(status.as_u16(), 200);

    // Should get either RUN_FINISHED or RUN_ERROR.
    // The cancel token aborts the turn, but the HTTP connection to mock
    // stays open until the mock's delay expires. Give extra time.
    let terminal = sse
        .next_matching(
            |e| is_type(e, "RUN_FINISHED") || is_type(e, "RUN_ERROR"),
            Duration::from_secs(15),
        )
        .await;
    assert!(
        terminal.is_some(),
        "Expected RUN_FINISHED or RUN_ERROR after abort"
    );
}

// ── Tool use tests ───────────────────────────────────────────────

#[tokio::test]
async fn tool_use_emits_tool_call_events() {
    // First response: tool use calling nexus_read_file
    // Second response: text reply after getting tool result
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "nexus_read_file",
            "toolu_test_001",
            r#"{"description":"Reading test file","path":"/tmp/nexus-test-file.txt"}"#,
        )),
        MockResponse::Sse(mock_llm::text_response("Got the file contents")),
    ])
    .await;

    // Create the file the tool will try to read
    std::fs::write("/tmp/nexus-test-file.txt", "test file content").ok();

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;
    start_turn(&client, &conv_id, "Read a file").await;

    // Should see TOOL_CALL_START
    let tool_start = sse
        .expect_event_type("TOOL_CALL_START", Duration::from_secs(10))
        .await;
    assert_eq!(
        tool_start.get("toolCallName").and_then(|n| n.as_str()),
        Some("nexus_read_file")
    );

    // Should see TOOL_CALL_ARGS
    sse.expect_event_type("TOOL_CALL_ARGS", Duration::from_secs(5))
        .await;

    // Should see TOOL_CALL_END
    sse.expect_event_type("TOOL_CALL_END", Duration::from_secs(5))
        .await;

    // Should see TOOL_CALL_RESULT
    let result = sse
        .expect_event_type("TOOL_CALL_RESULT", Duration::from_secs(5))
        .await;
    assert!(
        result.get("content").is_some(),
        "TOOL_CALL_RESULT should have content: {result}"
    );

    // Should eventually finish with text response
    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // Cleanup
    std::fs::remove_file("/tmp/nexus-test-file.txt").ok();
}

// ── Multi-turn tests ─────────────────────────────────────────────

#[tokio::test]
async fn multi_turn_context_includes_history() {
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::text_response("First reply")),
        MockResponse::Sse(mock_llm::text_response("Second reply")),
    ])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;

    // First turn
    start_turn(&client, &conv_id, "Hello").await;
    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // Second turn
    start_turn(&client, &conv_id, "Remember what I said?").await;
    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // The mock captured requests — find the last one (from second turn)
    let requests = mock.captured_requests();
    assert!(
        requests.len() >= 2,
        "Expected at least 2 requests to mock, got {}",
        requests.len()
    );

    let last_req = requests.last().unwrap();
    let messages = last_req["messages"]
        .as_array()
        .expect("last request should have messages array");

    // Should have more messages than just the new user message
    // (at minimum: original user + assistant + state_update + new user)
    assert!(
        messages.len() >= 3,
        "Second turn request should include conversation history, got {} messages",
        messages.len()
    );
}
