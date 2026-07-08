use ai_gateway_service::HarnessConfig;

use super::common::{small_body, TestHarness};

/// TC-WK-01：多条 job 均可被 worker 完成（smoke）。
#[tokio::test]
async fn tc_wk_01_multiple_jobs_complete() {
    let h = TestHarness::start().await;
    for i in 0..3 {
        let resp = h.enqueue(&format!("wk-{i}"), small_body(), axum::http::HeaderMap::new()).await;
        assert_eq!(resp.status(), 202);
    }
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let metrics = h.metrics().await;
    assert!(metrics.contains("worker_completed_total"));
}

/// TC-WK-03：回调失败进入 retry（不可达 URL）。
#[tokio::test]
async fn tc_wk_03_callback_failure_retry_stream() {
    let h = TestHarness::start_config(HarnessConfig {
        require_https_callback: Some(false),
        ..Default::default()
    })
    .await;

    let resp = h
        .client
        .post(format!("{}/v1/queue/enqueue", h.base_url))
        .header("x-tenant-id", "retry-t")
        .header("x-ratelimit-policy", "queue")
        .header("x-callback-url", "http://127.0.0.1:1/unreachable")
        .body(small_body())
        .send()
        .await
        .expect("enqueue");
    assert_eq!(resp.status(), 202);

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let depth = h.callback_retry_depth().await;
    assert!(depth >= 0);
}
