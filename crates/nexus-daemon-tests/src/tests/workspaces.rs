use crate::fixtures;
use crate::harness::TestDaemon;
use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn list_initially_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/workspaces").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn create_returns_201() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c
        .post("/api/workspaces", &fixtures::workspace_body("Test", "/tmp"))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "Test");
    assert_eq!(body["path"], "/tmp");
}

#[tokio::test]
async fn update_workspace() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/workspaces", &fixtures::workspace_body("Original", "/tmp"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c
        .put(
            &format!("/api/workspaces/{id}"),
            &json!({ "name": "Renamed" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Verify via list
    let (_, body) = c.get("/api/workspaces").await;
    let ws = &body.as_array().unwrap()[0];
    assert_eq!(ws["name"], "Renamed");
}

#[tokio::test]
async fn delete_returns_204() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/workspaces", &fixtures::workspace_body("Delete Me", "/tmp"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c.delete(&format!("/api/workspaces/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = c.get("/api/workspaces").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}
