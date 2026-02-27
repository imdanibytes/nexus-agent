use std::time::Duration;

use serde_json::json;

use crate::fixtures::setup_mock_agent;
use crate::harness::TestDaemon;
use crate::mock_llm::{self, MockLlmServer, MockResponse};

fn is_custom(event: &serde_json::Value, name: &str) -> bool {
    event.get("type").and_then(|t| t.as_str()) == Some("CUSTOM")
        && event.get("name").and_then(|n| n.as_str()) == Some(name)
}

#[tokio::test]
async fn multiple_subscribers_receive_same_event() {
    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();

    let mut sse1 = d.sse();
    let mut sse2 = d.sse();
    sse1.expect_sync().await;
    sse2.expect_sync().await;

    // Create a conversation — both subscribers should see thread_created
    client.post("/api/conversations", &json!({})).await;

    let ev1 = sse1
        .next_matching(|e| is_custom(e, "thread_created"), Duration::from_secs(5))
        .await;
    let ev2 = sse2
        .next_matching(|e| is_custom(e, "thread_created"), Duration::from_secs(5))
        .await;

    assert!(ev1.is_some(), "Subscriber 1 should receive thread_created");
    assert!(ev2.is_some(), "Subscriber 2 should receive thread_created");
}

#[tokio::test]
async fn sync_event_includes_active_run() {
    let mock = MockLlmServer::start(vec![MockResponse::Delayed {
        delay_ms: 5_000,
        sse: mock_llm::text_response("Slow response"),
    }])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();

    // Set up mock agent and start a turn
    let mut sse1 = d.sse();
    sse1.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;

    // Start a turn (will be delayed)
    client
        .post(
            "/api/chat",
            &json!({
                "conversationId": &conv_id,
                "message": "Start a slow turn"
            }),
        )
        .await;

    // Wait for RUN_STARTED to confirm the turn is active
    sse1.expect_event_type("RUN_STARTED", Duration::from_secs(5))
        .await;

    // Now connect a second SSE subscriber — its SYNC should include the active run
    let mut sse2 = d.sse();
    let sync = sse2.expect_sync().await;

    let active_runs = sync["activeRuns"]
        .as_array()
        .expect("SYNC should have activeRuns array");

    assert!(
        !active_runs.is_empty(),
        "SYNC activeRuns should include the running turn, got: {active_runs:?}"
    );

    // Abort to clean up
    client
        .post("/api/chat/abort", &json!({ "conversationId": &conv_id }))
        .await;
}
