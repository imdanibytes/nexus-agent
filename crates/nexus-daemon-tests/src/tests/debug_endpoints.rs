use std::time::Duration;

use serde_json::json;

use crate::harness::TestDaemon;

#[tokio::test]
async fn task_state_preset_emits_event() {
    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    // Create conversation for the task state
    let (_, conv) = client.post("/api/conversations", &json!({})).await;
    let conv_id = conv["id"].as_str().unwrap();

    // Set task state to "planning" preset
    let (status, body) = client
        .post(
            &format!("/api/debug/task-state/{conv_id}"),
            &json!({ "preset": "planning" }),
        )
        .await;
    assert_eq!(status.as_u16(), 200, "set_task_state failed: {body}");
    assert_eq!(body["ok"].as_bool(), Some(true));

    // Should emit task_state_changed
    let event = sse.expect_custom("task_state_changed", Duration::from_secs(5)).await;
    let value = &event["value"];
    assert_eq!(value["conversationId"].as_str(), Some(conv_id));
    assert!(value.get("plan").is_some(), "Should include plan");
    assert!(value.get("tasks").is_some(), "Should include tasks");
    assert_eq!(value["mode"].as_str(), Some("planning"));
}

#[tokio::test]
async fn emit_event_arrives_on_sse() {
    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    // Create a conversation for the thread_id
    let (_, conv) = client.post("/api/conversations", &json!({})).await;
    let conv_id = conv["id"].as_str().unwrap();

    // Drain the thread_created event
    sse.next_matching(
        |e| {
            e.get("type").and_then(|t| t.as_str()) == Some("CUSTOM")
                && e.get("name").and_then(|n| n.as_str()) == Some("thread_created")
        },
        Duration::from_secs(3),
    )
    .await;

    // Emit a custom event
    let (status, _) = client
        .post(
            "/api/debug/emit",
            &json!({
                "thread_id": conv_id,
                "name": "test_custom_event",
                "value": { "foo": "bar", "count": 42 }
            }),
        )
        .await;
    assert_eq!(status.as_u16(), 200);

    // Should arrive on SSE
    let event = sse
        .expect_custom("test_custom_event", Duration::from_secs(5))
        .await;
    assert_eq!(event["value"]["foo"].as_str(), Some("bar"));
    assert_eq!(event["value"]["count"].as_i64(), Some(42));
    assert_eq!(event["threadId"].as_str(), Some(conv_id));
}

#[tokio::test]
async fn force_compact_seals_span() {
    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();

    // Create conversation
    let (_, conv) = client.post("/api/conversations", &json!({})).await;
    let conv_id = conv["id"].as_str().unwrap();

    // Force compact (should indicate not enough messages)
    let (status, body) = client
        .post(
            &format!("/api/debug/compact/{conv_id}"),
            &json!({ "keep_recent": 2 }),
        )
        .await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(
        body["compacted"].as_bool(),
        Some(false),
        "Empty conversation shouldn't compact"
    );
}
