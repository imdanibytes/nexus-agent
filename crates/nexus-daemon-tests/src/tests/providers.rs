use crate::fixtures;
use crate::harness::TestDaemon;
use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn list_initially_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/providers").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn create_returns_201() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.post("/api/providers", &fixtures::provider_body("Test Provider")).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "Test Provider");
    assert_eq!(body["has_api_key"], true);
}

#[tokio::test]
async fn get_by_id() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post("/api/providers", &fixtures::provider_body("My Provider")).await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = c.get(&format!("/api/providers/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);
    assert_eq!(body["name"], "My Provider");
}

#[tokio::test]
async fn list_includes_created() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    c.post("/api/providers", &fixtures::provider_body("P1")).await;
    c.post("/api/providers", &fixtures::provider_body("P2")).await;

    let (_, body) = c.get("/api/providers").await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn update_name() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post("/api/providers", &fixtures::provider_body("Original")).await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c
        .put(&format!("/api/providers/{id}"), &json!({ "name": "Renamed" }))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = c.get(&format!("/api/providers/{id}")).await;
    assert_eq!(body["name"], "Renamed");
}

#[tokio::test]
async fn delete_returns_204() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post("/api/providers", &fixtures::provider_body("Delete Me")).await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c.delete(&format!("/api/providers/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn get_nonexistent_returns_404() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _) = c.get("/api/providers/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_key_not_leaked_in_list() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    c.post("/api/providers", &fixtures::provider_body("Secret")).await;

    let (_, body) = c.get("/api/providers").await;
    let item = &body.as_array().unwrap()[0];
    assert!(item.get("api_key").is_none());
    assert_eq!(item["has_api_key"], true);
}

#[tokio::test]
async fn api_key_not_leaked_in_get() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c.post("/api/providers", &fixtures::provider_body("Secret")).await;
    let id = created["id"].as_str().unwrap();

    let (_, body) = c.get(&format!("/api/providers/{id}")).await;
    assert!(body.get("api_key").is_none());
    assert_eq!(body["has_api_key"], true);
}
