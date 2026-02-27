use crate::fixtures;
use crate::harness::TestDaemon;
use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn get_nonexistent_conversation_returns_404() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _) = c.get("/api/conversations/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_nonexistent_provider_returns_404() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _) = c.get("/api/providers/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_agent_with_invalid_provider() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _) = c
        .post(
            "/api/agents",
            &fixtures::agent_body("Bad Agent", "nonexistent-provider"),
        )
        .await;
    assert!(status.is_client_error() || status.is_server_error());
}

#[tokio::test]
async fn chat_start_without_conversation_id_fails() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _) = c
        .post("/api/chat", &json!({ "message": "hello" }))
        .await;
    // Missing conversationId — should fail
    assert!(status.is_client_error() || status.is_server_error());
}

#[tokio::test]
async fn chat_start_with_nonexistent_conversation() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _) = c
        .post(
            "/api/chat",
            &json!({
                "conversationId": "does-not-exist",
                "message": "hello"
            }),
        )
        .await;
    assert!(status.is_client_error() || status.is_server_error());
}

#[tokio::test]
async fn abort_on_idle_conversation_returns_ok() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    // Abort when nothing is running — should still be 200
    let (status, _) = c
        .post(
            "/api/chat/abort",
            &json!({ "conversationId": "whatever" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn switch_path_with_invalid_message_id() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post_empty("/api/conversations").await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c
        .patch(
            &format!("/api/conversations/{id}/path"),
            &json!({ "messageId": "bogus-msg-id" }),
        )
        .await;
    // Should fail — message doesn't exist
    assert!(status.is_client_error() || status.is_server_error());
}
