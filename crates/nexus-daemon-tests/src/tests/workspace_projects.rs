use serde_json::json;

use crate::fixtures;
use crate::harness::TestDaemon;

#[tokio::test]
async fn workspace_with_projects() {
    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();

    // Create a project first
    let (status, proj) = client
        .post("/api/projects", &fixtures::project_body("my-project", "/tmp/test-project"))
        .await;
    assert_eq!(status.as_u16(), 201);
    let project_id = proj["id"].as_str().unwrap().to_string();

    // Create workspace with the project
    let (status, ws) = client
        .post(
            "/api/workspaces",
            &json!({
                "name": "My Workspace",
                "project_ids": [&project_id]
            }),
        )
        .await;
    assert_eq!(status.as_u16(), 201);
    let ws_id = ws["id"].as_str().unwrap().to_string();

    // List workspaces and find ours (no GET by ID endpoint)
    let (status, list) = client.get("/api/workspaces").await;
    assert_eq!(status.as_u16(), 200);
    let workspaces = list.as_array().expect("workspaces list");
    let ws = workspaces
        .iter()
        .find(|w| w["id"].as_str() == Some(ws_id.as_str()))
        .expect("workspace should be in list");
    let project_ids = ws["project_ids"]
        .as_array()
        .expect("workspace should have project_ids");
    assert!(
        project_ids.iter().any(|id| id.as_str() == Some(&project_id)),
        "Workspace should contain the project"
    );
}

#[tokio::test]
async fn set_and_get_active_workspace() {
    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();

    // Initially null
    let (status, body) = client.get("/api/workspaces/active").await;
    assert_eq!(status.as_u16(), 200);
    assert!(
        body.is_null() || body.get("id").is_none() || body["id"].is_null(),
        "Active workspace should be null initially"
    );

    // Create workspace
    let (_, ws) = client
        .post("/api/workspaces", &fixtures::workspace_body("Active WS"))
        .await;
    let ws_id = ws["id"].as_str().unwrap().to_string();

    // Set active
    let (status, _) = client
        .put("/api/workspaces/active", &json!({ "id": &ws_id }))
        .await;
    assert_eq!(status.as_u16(), 200);

    // Get active should return it
    let (status, body) = client.get("/api/workspaces/active").await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(body["id"].as_str(), Some(ws_id.as_str()));
}

#[tokio::test]
async fn new_conversation_stamped_with_active_workspace() {
    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();

    // Create and activate workspace
    let (_, ws) = client
        .post("/api/workspaces", &fixtures::workspace_body("Stamping WS"))
        .await;
    let ws_id = ws["id"].as_str().unwrap().to_string();
    client
        .put("/api/workspaces/active", &json!({ "id": &ws_id }))
        .await;

    // Create conversation — should be stamped
    let (status, conv) = client.post("/api/conversations", &json!({})).await;
    assert!(status.is_success(), "create conversation failed: {status}");
    let conv_id = conv["id"].as_str().unwrap();

    let (_, conv) = client.get(&format!("/api/conversations/{conv_id}")).await;
    assert_eq!(
        conv["workspace_id"].as_str(),
        Some(ws_id.as_str()),
        "Conversation should be stamped with active workspace ID"
    );
}
