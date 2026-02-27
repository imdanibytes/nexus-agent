use crate::fixtures;
use crate::harness::TestDaemon;
use reqwest::StatusCode;
use serde_json::json;

#[tokio::test]
async fn list_initially_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/mcp-servers").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn create_stdio_server() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c
        .post("/api/mcp-servers", &fixtures::mcp_server_body("Echo Server"))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "Echo Server");
}

#[tokio::test]
async fn create_http_server() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c
        .post(
            "/api/mcp-servers",
            &json!({
                "name": "HTTP Server",
                "url": "http://localhost:9999/mcp"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["url"], "http://localhost:9999/mcp");
}

#[tokio::test]
async fn update_server() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/mcp-servers", &fixtures::mcp_server_body("Original"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c
        .put(
            &format!("/api/mcp-servers/{id}"),
            &json!({ "name": "Renamed" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = c.get("/api/mcp-servers").await;
    let server = body
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .unwrap();
    assert_eq!(server["name"], "Renamed");
}

#[tokio::test]
async fn delete_returns_204() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (_, created) = c
        .post("/api/mcp-servers", &fixtures::mcp_server_body("Delete Me"))
        .await;
    let id = created["id"].as_str().unwrap();

    let (status, _) = c.delete(&format!("/api/mcp-servers/{id}")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = c.get("/api/mcp-servers").await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}
