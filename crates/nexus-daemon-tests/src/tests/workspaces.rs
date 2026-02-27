use crate::fixtures;
use crate::harness::TestDaemon;
use reqwest::StatusCode;
use serde_json::json;

// ── Project API (path-bearing, was /api/workspaces) ──

#[tokio::test]
async fn projects_list_initially_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/projects").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn projects_create_returns_201() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c
        .post("/api/projects", &fixtures::project_body("Test", "/tmp"))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "Test");
    assert_eq!(body["path"], "/tmp");
}

#[tokio::test]
async fn projects_update() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/projects", &fixtures::project_body("Original", "/tmp"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c
        .put(
            &format!("/api/projects/{id}"),
            &json!({ "name": "Renamed" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = c.get("/api/projects").await;
    let proj = &body.as_array().unwrap()[0];
    assert_eq!(proj["name"], "Renamed");
}

#[tokio::test]
async fn projects_delete_returns_204() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/projects", &fixtures::project_body("Delete Me", "/tmp"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c.delete(&format!("/api/projects/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = c.get("/api/projects").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

// ── Workspace API (logical grouping) ──

#[tokio::test]
async fn workspaces_list_initially_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/workspaces").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn workspaces_create_returns_201() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c
        .post("/api/workspaces", &fixtures::workspace_body("Auth Migration"))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "Auth Migration");
    assert!(body.get("path").is_none() || body["path"].is_null());
}

#[tokio::test]
async fn workspaces_update() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/workspaces", &fixtures::workspace_body("Original"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c
        .put(
            &format!("/api/workspaces/{id}"),
            &json!({ "name": "Renamed", "description": "Updated desc" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = c.get("/api/workspaces").await;
    let ws = &body.as_array().unwrap()[0];
    assert_eq!(ws["name"], "Renamed");
    assert_eq!(ws["description"], "Updated desc");
}

#[tokio::test]
async fn workspaces_delete_returns_204() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/workspaces", &fixtures::workspace_body("Delete Me"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c.delete(&format!("/api/workspaces/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = c.get("/api/workspaces").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn workspaces_active_initially_null() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/workspaces/active").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_null());
}

#[tokio::test]
async fn workspaces_set_active() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/workspaces", &fixtures::workspace_body("My Workspace"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c
        .put("/api/workspaces/active", &json!({ "id": id }))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, active) = c.get("/api/workspaces/active").await;
    assert_eq!(active["id"], id);
    assert_eq!(active["name"], "My Workspace");
}
