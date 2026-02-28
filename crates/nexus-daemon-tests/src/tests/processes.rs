use std::time::Duration;

use serde_json::json;

use crate::fixtures::setup_mock_agent;
use crate::harness::TestDaemon;
use crate::mock_llm::{self, MockLlmServer, MockResponse};

#[tokio::test]
async fn list_processes_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    // Create a conversation to have a valid ID
    let (status, conv) = c
        .post("/api/conversations", &json!({ "title": "test" }))
        .await;
    assert_eq!(status.as_u16(), 201);
    let conv_id = conv["id"].as_str().unwrap();

    let (status, body) = c.get(&format!("/api/processes/{conv_id}")).await;
    assert_eq!(status.as_u16(), 200);
    assert!(body.is_array(), "should return array: {body}");
    assert!(
        body.as_array().unwrap().is_empty(),
        "should be empty initially"
    );
}

#[tokio::test]
async fn list_processes_nonexistent_conversation_returns_empty() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    // Non-existent conversation — handler doesn't 404, just returns empty list
    let (status, body) = c.get("/api/processes/does-not-exist").await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn stop_nonexistent_process_returns_404() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, _body) = c.post_empty("/api/processes/fake-id/stop").await;
    assert_eq!(status.as_u16(), 404);
}

#[tokio::test]
async fn bg_process_appears_in_list() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    // Set up a mock agent that spawns a background bash process
    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "bash",
            "toolu_bg_001",
            r#"{"description":"Long running","command":"sleep 30","run_in_background":true}"#,
        )),
        MockResponse::Sse(mock_llm::text_response("Started background process")),
    ])
    .await;

    let (_, _, conv_id) = setup_mock_agent(&c, &mock.url).await;

    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post(
        "/api/chat",
        &json!({
            "conversationId": conv_id,
            "message": "Run a background task"
        }),
    )
    .await;

    // Wait for the turn to finish so the process is registered
    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(15))
        .await;

    // List processes — should have at least one
    let (status, body) = c.get(&format!("/api/processes/{conv_id}")).await;
    assert_eq!(status.as_u16(), 200);

    let processes = body.as_array().expect("processes should be array");
    assert!(
        !processes.is_empty(),
        "should have a background process: {body}"
    );

    let proc = &processes[0];
    assert!(proc["id"].is_string(), "process should have id");
    assert_eq!(proc["conversationId"], conv_id);
    assert!(proc["command"].is_string(), "process should have command");
    assert!(proc["kind"].is_string(), "process should have kind");
    assert!(proc["status"].is_string(), "process should have status");
    assert!(
        proc["startedAt"].is_string(),
        "process should have startedAt"
    );
}

#[tokio::test]
async fn stop_running_process_returns_ok() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let mock = MockLlmServer::start(vec![
        MockResponse::Sse(mock_llm::tool_use_response(
            "bash",
            "toolu_bg_002",
            r#"{"description":"Long running","command":"sleep 60","run_in_background":true}"#,
        )),
        MockResponse::Sse(mock_llm::text_response("Started")),
    ])
    .await;

    let (_, _, conv_id) = setup_mock_agent(&c, &mock.url).await;

    let mut sse = d.sse();
    sse.expect_sync().await;

    c.post(
        "/api/chat",
        &json!({
            "conversationId": conv_id,
            "message": "Run background"
        }),
    )
    .await;

    sse.expect_event_type("RUN_FINISHED", Duration::from_secs(15))
        .await;

    // Get the process ID
    let (_, procs) = c.get(&format!("/api/processes/{conv_id}")).await;
    let processes = procs.as_array().expect("processes array");
    assert!(!processes.is_empty(), "need a process to stop");

    let process_id = processes[0]["id"].as_str().unwrap();
    let proc_status = processes[0]["status"].as_str().unwrap();

    // Only attempt stop if still running
    if proc_status == "running" {
        let (stop_status, _) = c
            .post_empty(&format!("/api/processes/{process_id}/stop"))
            .await;
        assert_eq!(stop_status.as_u16(), 200);

        // Verify it's no longer running
        let (_, updated_procs) = c.get(&format!("/api/processes/{conv_id}")).await;
        let updated = updated_procs
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == process_id)
            .expect("process should still be in list");
        assert_ne!(
            updated["status"], "running",
            "process should no longer be running after stop"
        );
    }
}
