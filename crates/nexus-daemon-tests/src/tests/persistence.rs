use serde_json::json;

use crate::fixtures;
use crate::harness::TestDaemon;

#[tokio::test]
async fn conversations_survive_restart() {
    let home_path = tempfile::tempdir().unwrap();
    let home = home_path.path().to_path_buf();

    // First daemon: create a conversation
    let conv_id = {
        let d = TestDaemon::spawn_at_path(home.clone()).await.unwrap();
        let client = d.client();

        let (status, body) = client.post("/api/conversations", &json!({})).await;
        assert!(status.is_success(), "create conversation failed: {status} {body}");
        let id = body["id"].as_str().unwrap().to_string();

        // Rename it so we can verify content
        client
            .patch(
                &format!("/api/conversations/{id}"),
                &json!({ "title": "Survived Restart" }),
            )
            .await;

        id
        // daemon drops here → process killed
    };

    // Second daemon: same data directory, different port
    let d2 = TestDaemon::spawn_at_path(home).await.unwrap();
    let client2 = d2.client();

    let (status, body) = client2
        .get(&format!("/api/conversations/{conv_id}"))
        .await;
    assert_eq!(status.as_u16(), 200, "Conversation should survive restart");
    assert_eq!(
        body["title"].as_str(),
        Some("Survived Restart"),
        "Title should be preserved"
    );
}

#[tokio::test]
async fn providers_survive_restart() {
    let home_path = tempfile::tempdir().unwrap();
    let home = home_path.path().to_path_buf();

    let provider_id = {
        let d = TestDaemon::spawn_at_path(home.clone()).await.unwrap();
        let client = d.client();

        let (status, body) = client
            .post("/api/providers", &fixtures::provider_body("Persistent Provider"))
            .await;
        assert_eq!(status.as_u16(), 201);
        body["id"].as_str().unwrap().to_string()
    };

    let d2 = TestDaemon::spawn_at_path(home).await.unwrap();
    let client2 = d2.client();

    let (status, body) = client2
        .get(&format!("/api/providers/{provider_id}"))
        .await;
    assert_eq!(status.as_u16(), 200, "Provider should survive restart");
    assert_eq!(body["name"].as_str(), Some("Persistent Provider"));
}

#[tokio::test]
async fn agents_survive_restart() {
    let home_path = tempfile::tempdir().unwrap();
    let home = home_path.path().to_path_buf();

    let agent_id = {
        let d = TestDaemon::spawn_at_path(home.clone()).await.unwrap();
        let client = d.client();

        // Need a provider first
        let (_, prov) = client
            .post("/api/providers", &fixtures::provider_body("Agent Test Provider"))
            .await;
        let provider_id = prov["id"].as_str().unwrap();

        let (status, body) = client
            .post(
                "/api/agents",
                &fixtures::agent_body("Persistent Agent", provider_id),
            )
            .await;
        assert_eq!(status.as_u16(), 201);
        body["id"].as_str().unwrap().to_string()
    };

    let d2 = TestDaemon::spawn_at_path(home).await.unwrap();
    let client2 = d2.client();

    let (status, body) = client2.get(&format!("/api/agents/{agent_id}")).await;
    assert_eq!(status.as_u16(), 200, "Agent should survive restart");
    assert_eq!(body["name"].as_str(), Some("Persistent Agent"));
}
