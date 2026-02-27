use crate::harness::TestDaemon;
use reqwest::StatusCode;

#[tokio::test]
async fn browse_tmp_returns_entries() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/browse?path=/tmp").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["path"].is_string());
    assert!(body["entries"].is_array());
}

#[tokio::test]
async fn browse_without_path_uses_default() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/browse").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["path"].is_string());
    assert!(body["entries"].is_array());
}
