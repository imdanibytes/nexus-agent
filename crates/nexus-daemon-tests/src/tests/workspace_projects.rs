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
async fn new_conversation_starts_without_workspace() {
    let d = TestDaemon::spawn().await.unwrap();
    let client = d.client();

    // Create conversation — should have no workspace
    let (status, conv) = client.post("/api/conversations", &json!({})).await;
    assert!(status.is_success(), "create conversation failed: {status}");
    let conv_id = conv["id"].as_str().unwrap();

    let (_, conv) = client.get(&format!("/api/conversations/{conv_id}")).await;
    assert!(
        conv["workspace_id"].is_null(),
        "New conversation should have no workspace"
    );
}
