use crate::harness::TestDaemon;
use reqwest::StatusCode;

#[tokio::test]
async fn list_tools_returns_array() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/tools").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["tools"].is_array());
}
