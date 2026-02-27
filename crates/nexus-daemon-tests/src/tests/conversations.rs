use crate::harness::TestDaemon;
use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn list_initially_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/conversations").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn create_returns_conversation() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.post_empty("/api/conversations").await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn create_with_explicit_id() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.post("/api/conversations", &json!({ "id": "my-conv" })).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["id"], "my-conv");
}

#[tokio::test]
async fn get_returns_created_conversation() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post_empty("/api/conversations").await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = c.get(&format!("/api/conversations/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);
}

#[tokio::test]
async fn list_includes_created_conversations() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    c.post_empty("/api/conversations").await;
    c.post_empty("/api/conversations").await;

    let (_, body) = c.get("/api/conversations").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn delete_returns_204() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post_empty("/api/conversations").await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c.delete(&format!("/api/conversations/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn get_after_delete_returns_404() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post_empty("/api/conversations").await;
    let id = created["id"].as_str().unwrap();

    c.delete(&format!("/api/conversations/{id}")).await;

    let (status, _) = c.get(&format!("/api/conversations/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rename_conversation() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post_empty("/api/conversations").await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c
        .patch(
            &format!("/api/conversations/{id}"),
            &json!({ "title": "New Title" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = c.get(&format!("/api/conversations/{id}")).await;
    assert_eq!(body["title"], "New Title");
}

#[tokio::test]
async fn list_returns_meta_with_message_count() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    c.post_empty("/api/conversations").await;

    let (_, body) = c.get("/api/conversations").await;
    let item = &body.as_array().unwrap()[0];
    assert!(item["id"].is_string());
    assert!(item["created_at"].is_string());
    assert!(item["updated_at"].is_string());
    // message_count should be 0 for a fresh conversation
    assert_eq!(item["message_count"], 0);
}
