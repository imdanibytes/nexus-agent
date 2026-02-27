use crate::fixtures;
use crate::harness::TestDaemon;
use reqwest::StatusCode;
use serde_json::json;

/// Helper: create a provider and return its id.
async fn create_provider(c: &crate::client::DaemonClient) -> String {
    let (_, body) = c
        .post("/api/providers", &fixtures::provider_body("Test Provider"))
        .await;
    body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn list_initially_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/agents").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn create_with_valid_provider() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let pid = create_provider(&c).await;

    let (status, body) = c
        .post("/api/agents", &fixtures::agent_body("My Agent", &pid))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "My Agent");
    assert_eq!(body["provider_id"], pid);
}

#[tokio::test]
async fn create_with_invalid_provider_fails() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _) = c
        .post("/api/agents", &fixtures::agent_body("Bad Agent", "nonexistent"))
        .await;
    // Should reject — provider doesn't exist
    assert!(status.is_client_error() || status.is_server_error());
}

#[tokio::test]
async fn get_active_initially_null() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/agents/active").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["agent_id"].is_null());
}

#[tokio::test]
async fn set_and_get_active() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let pid = create_provider(&c).await;

    let (_, agent) = c
        .post("/api/agents", &fixtures::agent_body("Active Agent", &pid))
        .await;
    let agent_id = agent["id"].as_str().unwrap();

    let (status, _) = c
        .put("/api/agents/active", &json!({ "agent_id": agent_id }))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = c.get("/api/agents/active").await;
    assert_eq!(body["agent_id"], agent_id);
}

#[tokio::test]
async fn delete_returns_204() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let pid = create_provider(&c).await;

    let (_, agent) = c
        .post("/api/agents", &fixtures::agent_body("Delete Me", &pid))
        .await;
    let id = agent["id"].as_str().unwrap();

    let (status, _) = c.delete(&format!("/api/agents/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn update_agent_fields() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let pid = create_provider(&c).await;

    let (_, agent) = c
        .post("/api/agents", &fixtures::agent_body("Original", &pid))
        .await;
    let id = agent["id"].as_str().unwrap();

    let (status, _) = c
        .put(
            &format!("/api/agents/{id}"),
            &json!({ "name": "Renamed Agent" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = c.get(&format!("/api/agents/{id}")).await;
    assert_eq!(body["name"], "Renamed Agent");
}

#[tokio::test]
async fn list_includes_created() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let pid = create_provider(&c).await;

    c.post("/api/agents", &fixtures::agent_body("A1", &pid)).await;
    c.post("/api/agents", &fixtures::agent_body("A2", &pid)).await;

    let (_, body) = c.get("/api/agents").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}
