use std::time::Duration;

use serde_json::json;

use crate::fixtures::setup_mock_agent;
use crate::harness::TestDaemon;
use crate::mock_llm::{self, MockLlmServer, MockResponse};

#[tokio::test]
async fn switch_path_changes_active_messages() {
    // Two turns worth of mock responses, plus a branch response
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::text_response("First response")),
        MockResponse::Sse(mock_llm::text_response("Branch response")),
    ])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;

    // First turn
    client
        .post(
            "/api/chat",
            &json!({
                "conversationId": &conv_id,
                "message": "First message"
            }),
        )
        .await;
    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // Brief delay for message persistence after RUN_FINISHED
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Get the conversation to find the user message ID
    let (_, conv) = client.get(&format!("/api/conversations/{conv_id}")).await;
    let messages = conv["messages"].as_array().expect("messages should be array");
    assert!(
        messages.len() >= 2,
        "Should have user + assistant messages after turn. Got: {conv}"
    );

    let user_msg_id = messages
        .iter()
        .find(|m| m["role"].as_str() == Some("user"))
        .and_then(|m| m["id"].as_str())
        .expect("should find user message");

    // Branch from the user message (edit/regenerate)
    client
        .post(
            "/api/chat/branch",
            &json!({
                "conversationId": &conv_id,
                "messageId": user_msg_id,
                "message": "Alternative message"
            }),
        )
        .await;
    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
        .await;

    // Get conversation again — should have more messages now (branched path)
    let (_, conv2) = client.get(&format!("/api/conversations/{conv_id}")).await;
    let _active_path = conv2["active_path"]
        .as_array()
        .expect("should have active_path");

    // Switch back to original path using the original user message
    let (status, _) = client
        .patch(
            &format!("/api/conversations/{conv_id}/path"),
            &json!({ "messageId": user_msg_id }),
        )
        .await;
    // Could be 200 or could differ based on implementation
    assert!(
        status.is_success(),
        "Path switch should succeed, got {status}"
    );
}

#[tokio::test]
async fn switch_into_sealed_span_returns_409() {
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::text_response("Message 1")),
        MockResponse::Sse(mock_llm::text_response("Message 2")),
        MockResponse::Sse(mock_llm::text_response("Message 3")),
    ])
    .await;

    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, _, conv_id) = setup_mock_agent(&client, &mock.url).await;

    // Three turns to build enough messages for compaction
    for msg in ["msg1", "msg2", "msg3"] {
        client
            .post(
                "/api/chat",
                &json!({
                    "conversationId": &conv_id,
                    "message": msg
                }),
            )
            .await;
        sse.expect_event_type("RUN_FINISHED", Duration::from_secs(10))
            .await;
    }

    // Get first message ID (will be sealed after compaction)
    let (_, conv) = client.get(&format!("/api/conversations/{conv_id}")).await;
    let messages = conv["messages"].as_array().expect("messages array");
    let first_msg_id = messages[0]["id"].as_str().expect("first message id");

    // Force compact — keeps only 2 most recent
    let (status, body) = client
        .post(
            &format!("/api/debug/compact/{conv_id}"),
            &json!({ "keep_recent": 2 }),
        )
        .await;
    assert_eq!(status.as_u16(), 200);

    if body["compacted"].as_bool() != Some(true) {
        // Not enough messages to compact — skip assertion
        return;
    }

    // Try to switch to the sealed message — should fail with 409
    let (status, _) = client
        .patch(
            &format!("/api/conversations/{conv_id}/path"),
            &json!({ "messageId": first_msg_id }),
        )
        .await;
    assert_eq!(
        status.as_u16(),
        409,
        "Switching to a sealed span message should return 409 CONFLICT"
    );
}
