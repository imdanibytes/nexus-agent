//! Integration tests verifying that service-layer mutations emit
//! SSE events with correct names (no `data:` prefix).

use std::time::Duration;

use serde_json::{json, Value};

use crate::fixtures;
use crate::harness::TestDaemon;

/// Check if a CUSTOM event matches the expected name.
fn is_custom(event: &Value, expected_name: &str) -> bool {
    event.get("type").and_then(|t| t.as_str()) == Some("CUSTOM")
        && event.get("name").and_then(|n| n.as_str()) == Some(expected_name)
}

/// Helper: create a provider and return its id.
async fn create_provider(c: &crate::client::DaemonClient) -> String {
    let (_, body) = c
        .post("/api/providers", &fixtures::provider_body("Test Provider"))
        .await;
    body["id"].as_str().unwrap().to_string()
}

// ── Thread events ──

#[tokio::test]
async fn create_conversation_emits_thread_created() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post_empty("/api/conversations").await;

    let event = sse
        .next_matching(|e| is_custom(e, "thread_created"), Duration::from_secs(3))
        .await;
    assert!(event.is_some(), "Expected 'thread_created' CUSTOM event");
}

#[tokio::test]
async fn delete_conversation_emits_thread_deleted() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, body) = c.post_empty("/api/conversations").await;
    let id = body["id"].as_str().unwrap();

    // Drain the thread_created event
    sse.next_matching(|e| is_custom(e, "thread_created"), Duration::from_secs(3))
        .await;

    c.delete(&format!("/api/conversations/{id}")).await;

    let event = sse
        .next_matching(|e| is_custom(e, "thread_deleted"), Duration::from_secs(3))
        .await;
    assert!(event.is_some(), "Expected 'thread_deleted' CUSTOM event");
}

#[tokio::test]
async fn rename_conversation_emits_title_update() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, body) = c.post_empty("/api/conversations").await;
    let id = body["id"].as_str().unwrap();

    // Drain the thread_created event
    sse.next_matching(|e| is_custom(e, "thread_created"), Duration::from_secs(3))
        .await;

    c.patch(
        &format!("/api/conversations/{id}"),
        &json!({ "title": "Renamed" }),
    )
    .await;

    let event = sse
        .next_matching(|e| is_custom(e, "title_update"), Duration::from_secs(3))
        .await;
    assert!(event.is_some(), "Expected 'title_update' CUSTOM event");
}

// ── Agent events ──

#[tokio::test]
async fn create_agent_emits_agent_created() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let pid = create_provider(&c).await;

    c.post("/api/agents", &fixtures::agent_body("Test Agent", &pid))
        .await;

    let event = sse
        .next_matching(|e| is_custom(e, "agent_created"), Duration::from_secs(3))
        .await;
    assert!(event.is_some(), "Expected 'agent_created' CUSTOM event");
}

#[tokio::test]
async fn update_agent_emits_agent_updated() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let pid = create_provider(&c).await;
    let (_, agent) = c
        .post("/api/agents", &fixtures::agent_body("Test Agent", &pid))
        .await;
    let id = agent["id"].as_str().unwrap();

    // Drain agent_created
    sse.next_matching(|e| is_custom(e, "agent_created"), Duration::from_secs(3))
        .await;

    c.put(
        &format!("/api/agents/{id}"),
        &json!({ "name": "Renamed Agent" }),
    )
    .await;

    let event = sse
        .next_matching(|e| is_custom(e, "agent_updated"), Duration::from_secs(3))
        .await;
    assert!(event.is_some(), "Expected 'agent_updated' CUSTOM event");
}

#[tokio::test]
async fn delete_agent_emits_agent_deleted() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let pid = create_provider(&c).await;
    let (_, agent) = c
        .post("/api/agents", &fixtures::agent_body("Test Agent", &pid))
        .await;
    let id = agent["id"].as_str().unwrap();

    // Drain agent_created
    sse.next_matching(|e| is_custom(e, "agent_created"), Duration::from_secs(3))
        .await;

    c.delete(&format!("/api/agents/{id}")).await;

    let event = sse
        .next_matching(|e| is_custom(e, "agent_deleted"), Duration::from_secs(3))
        .await;
    assert!(event.is_some(), "Expected 'agent_deleted' CUSTOM event");
}

#[tokio::test]
async fn set_active_agent_emits_active_agent_changed() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let pid = create_provider(&c).await;
    let (_, agent) = c
        .post("/api/agents", &fixtures::agent_body("Test Agent", &pid))
        .await;
    let agent_id = agent["id"].as_str().unwrap();

    // Drain agent_created
    sse.next_matching(|e| is_custom(e, "agent_created"), Duration::from_secs(3))
        .await;

    c.put("/api/agents/active", &json!({ "agent_id": agent_id }))
        .await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "active_agent_changed"),
            Duration::from_secs(3),
        )
        .await;
    assert!(
        event.is_some(),
        "Expected 'active_agent_changed' CUSTOM event"
    );
}

// ── Provider events ──

#[tokio::test]
async fn create_provider_emits_provider_created() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post("/api/providers", &fixtures::provider_body("Test Provider"))
        .await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "provider_created"),
            Duration::from_secs(3),
        )
        .await;
    assert!(event.is_some(), "Expected 'provider_created' CUSTOM event");
}

#[tokio::test]
async fn update_provider_emits_provider_updated() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, prov) = c
        .post("/api/providers", &fixtures::provider_body("Test Provider"))
        .await;
    let id = prov["id"].as_str().unwrap();

    // Drain provider_created
    sse.next_matching(
        |e| is_custom(e, "provider_created"),
        Duration::from_secs(3),
    )
    .await;

    c.put(
        &format!("/api/providers/{id}"),
        &json!({ "name": "Renamed Provider" }),
    )
    .await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "provider_updated"),
            Duration::from_secs(3),
        )
        .await;
    assert!(event.is_some(), "Expected 'provider_updated' CUSTOM event");
}

#[tokio::test]
async fn delete_provider_emits_provider_deleted() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, prov) = c
        .post("/api/providers", &fixtures::provider_body("Test Provider"))
        .await;
    let id = prov["id"].as_str().unwrap();

    // Drain provider_created
    sse.next_matching(
        |e| is_custom(e, "provider_created"),
        Duration::from_secs(3),
    )
    .await;

    c.delete(&format!("/api/providers/{id}")).await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "provider_deleted"),
            Duration::from_secs(3),
        )
        .await;
    assert!(event.is_some(), "Expected 'provider_deleted' CUSTOM event");
}

// ── Project events ──

#[tokio::test]
async fn create_project_emits_project_created() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post("/api/projects", &fixtures::project_body("Test Proj", "/tmp"))
        .await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "project_created"),
            Duration::from_secs(3),
        )
        .await;
    assert!(
        event.is_some(),
        "Expected 'project_created' CUSTOM event"
    );
}

#[tokio::test]
async fn update_project_emits_project_updated() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, proj) = c
        .post("/api/projects", &fixtures::project_body("Test Proj", "/tmp"))
        .await;
    let id = proj["id"].as_str().unwrap();

    // Drain project_created
    sse.next_matching(
        |e| is_custom(e, "project_created"),
        Duration::from_secs(3),
    )
    .await;

    c.put(
        &format!("/api/projects/{id}"),
        &json!({ "name": "Renamed Proj" }),
    )
    .await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "project_updated"),
            Duration::from_secs(3),
        )
        .await;
    assert!(
        event.is_some(),
        "Expected 'project_updated' CUSTOM event"
    );
}

#[tokio::test]
async fn delete_project_emits_project_deleted() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, proj) = c
        .post("/api/projects", &fixtures::project_body("Test Proj", "/tmp"))
        .await;
    let id = proj["id"].as_str().unwrap();

    // Drain project_created
    sse.next_matching(
        |e| is_custom(e, "project_created"),
        Duration::from_secs(3),
    )
    .await;

    c.delete(&format!("/api/projects/{id}")).await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "project_deleted"),
            Duration::from_secs(3),
        )
        .await;
    assert!(
        event.is_some(),
        "Expected 'project_deleted' CUSTOM event"
    );
}

// ── Workspace events ──

#[tokio::test]
async fn create_workspace_emits_workspace_created() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post("/api/workspaces", &fixtures::workspace_body("Test WS"))
        .await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "workspace_created"),
            Duration::from_secs(3),
        )
        .await;
    assert!(
        event.is_some(),
        "Expected 'workspace_created' CUSTOM event"
    );
}

#[tokio::test]
async fn update_workspace_emits_workspace_updated() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, ws) = c
        .post("/api/workspaces", &fixtures::workspace_body("Test WS"))
        .await;
    let id = ws["id"].as_str().unwrap();

    // Drain workspace_created
    sse.next_matching(
        |e| is_custom(e, "workspace_created"),
        Duration::from_secs(3),
    )
    .await;

    c.put(
        &format!("/api/workspaces/{id}"),
        &json!({ "name": "Renamed WS" }),
    )
    .await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "workspace_updated"),
            Duration::from_secs(3),
        )
        .await;
    assert!(
        event.is_some(),
        "Expected 'workspace_updated' CUSTOM event"
    );
}

#[tokio::test]
async fn delete_workspace_emits_workspace_deleted() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, ws) = c
        .post("/api/workspaces", &fixtures::workspace_body("Test WS"))
        .await;
    let id = ws["id"].as_str().unwrap();

    // Drain workspace_created
    sse.next_matching(
        |e| is_custom(e, "workspace_created"),
        Duration::from_secs(3),
    )
    .await;

    c.delete(&format!("/api/workspaces/{id}")).await;

    let event = sse
        .next_matching(
            |e| is_custom(e, "workspace_deleted"),
            Duration::from_secs(3),
        )
        .await;
    assert!(
        event.is_some(),
        "Expected 'workspace_deleted' CUSTOM event"
    );
}

// ── Meta tests ──

#[tokio::test]
async fn rename_emits_single_title_update() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    let (_, body) = c.post_empty("/api/conversations").await;
    let id = body["id"].as_str().unwrap();

    // Drain the thread_created event
    sse.next_matching(|e| is_custom(e, "thread_created"), Duration::from_secs(3))
        .await;

    c.patch(
        &format!("/api/conversations/{id}"),
        &json!({ "title": "New Title" }),
    )
    .await;

    // First title_update should arrive
    let first = sse
        .next_matching(|e| is_custom(e, "title_update"), Duration::from_secs(3))
        .await;
    assert!(first.is_some(), "Expected at least one 'title_update'");

    // No second title_update within 1 second
    let second = sse
        .next_matching(|e| is_custom(e, "title_update"), Duration::from_secs(1))
        .await;
    assert!(
        second.is_none(),
        "Expected exactly one 'title_update', got two"
    );
}

#[tokio::test]
async fn no_data_prefixed_events() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();
    let mut sse = d.sse();
    sse.expect_sync().await;

    // Trigger multiple mutations
    let pid = create_provider(&c).await;
    c.post("/api/agents", &fixtures::agent_body("A", &pid))
        .await;
    c.post_empty("/api/conversations").await;
    c.post("/api/workspaces", &fixtures::workspace_body("W"))
        .await;

    // Collect all CUSTOM events for 2 seconds
    let mut events = Vec::new();
    loop {
        match sse
            .next_matching(
                |e| e.get("type").and_then(|t| t.as_str()) == Some("CUSTOM"),
                Duration::from_secs(2),
            )
            .await
        {
            Some(e) => events.push(e),
            None => break,
        }
    }

    // None should have "data:" prefix
    for event in &events {
        let name = event["name"].as_str().unwrap_or("");
        assert!(
            !name.starts_with("data:"),
            "Found event with data: prefix: {name}"
        );
    }
    assert!(!events.is_empty(), "Expected at least some CUSTOM events");
}
