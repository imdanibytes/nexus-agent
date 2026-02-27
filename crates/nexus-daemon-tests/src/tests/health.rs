use crate::harness::TestDaemon;
use reqwest::StatusCode;

#[tokio::test]
async fn status_returns_ok() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    let (status, body) = c.get("/api/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn status_is_stable_across_multiple_calls() {
    let d = TestDaemon::spawn().await.unwrap();
    let c = d.client();

    for _ in 0..5 {
        let (status, _) = c.get("/api/status").await;
        assert_eq!(status, StatusCode::OK);
    }
}
