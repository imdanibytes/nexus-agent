use crate::harness::TestDaemon;
use reqwest::StatusCode;
use serde_json::json;
use std::time::Duration;

/// Chat tests require ANTHROPIC_API_KEY. They skip gracefully when it's not set.

#[tokio::test]
async fn start_turn_returns_immediately() {
    let Some(d) = TestDaemon::spawn_with_api_key().await.unwrap() else {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    };
    let c = d.client();

    // Create a conversation first
    let (_, conv) = c.post_empty("/api/conversations").await;
    let conv_id = conv["id"].as_str().unwrap();

    let (status, body) = c
        .post(
            "/api/chat",
            &json!({
                "conversationId": conv_id,
                "message": "Say exactly: hello"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
}

#[tokio::test]
async fn start_turn_emits_run_started() {
    let Some(d) = TestDaemon::spawn_with_api_key().await.unwrap() else {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    };
    let c = d.client();
    let mut sse = d.sse();

    // Drain SYNC
    sse.expect_sync().await;

    let (_, conv) = c.post_empty("/api/conversations").await;
    let conv_id = conv["id"].as_str().unwrap();

    c.post(
        "/api/chat",
        &json!({
            "conversationId": conv_id,
            "message": "Say exactly: hello"
        }),
    )
    .await;

    let event = sse
        .expect_event_type("RUN_STARTED", Duration::from_secs(10))
        .await;
    assert_eq!(event["threadId"], conv_id);
}

#[tokio::test]
async fn abort_turn_stops_run() {
    let Some(d) = TestDaemon::spawn_with_api_key().await.unwrap() else {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    };
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, conv) = c.post_empty("/api/conversations").await;
    let conv_id = conv["id"].as_str().unwrap();

    c.post(
        "/api/chat",
        &json!({
            "conversationId": conv_id,
            "message": "Write a very long essay about the history of computing"
        }),
    )
    .await;

    // Wait for run to start
    sse.expect_event_type("RUN_STARTED", Duration::from_secs(10))
        .await;

    // Abort it
    let (status, _) = c
        .post("/api/chat/abort", &json!({ "conversationId": conv_id }))
        .await;
    assert_eq!(status, StatusCode::OK);

    // Should get either RUN_FINISHED or RUN_ERROR
    let event = sse
        .next_matching(
            |e| {
                let ty = e.get("type").and_then(|t| t.as_str()).unwrap_or("");
                ty == "RUN_FINISHED" || ty == "RUN_ERROR"
            },
            Duration::from_secs(10),
        )
        .await;
    assert!(event.is_some(), "Expected RUN_FINISHED or RUN_ERROR after abort");
}
