use crate::harness::TestDaemon;

#[tokio::test]
async fn connects_and_receives_sync_event() {
    let d = TestDaemon::spawn().await.unwrap();
    let mut sse = d.sse();

    let sync = sse.expect_sync().await;
    assert_eq!(sync["type"], "SYNC");
    assert!(sync["activeRuns"].is_array());
}

#[tokio::test]
async fn sync_has_empty_active_runs_when_idle() {
    let d = TestDaemon::spawn().await.unwrap();
    let mut sse = d.sse();

    let sync = sse.expect_sync().await;
    assert_eq!(sync["activeRuns"].as_array().unwrap().len(), 0);
}
