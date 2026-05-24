use ai_gateway_service::HarnessConfig;

use super::common::TestHarness;

/// TC-MET-01：metrics 含 queue_depth / pel_size。
#[tokio::test]
async fn tc_met_01_metrics_endpoint() {
    let h = TestHarness::start().await;
    let body = h.metrics().await;
    assert!(body.contains("queue_depth"));
    assert!(body.contains("pel_size"));
    assert!(body.contains("enqueue_total"));
}

/// TC-MET-02：rate_limited 带标签（触发后）。
#[tokio::test]
async fn tc_met_02_labeled_rate_limited() {
    let h = TestHarness::start_config(HarnessConfig {
        rate_limit_burst: Some(1),
        ..Default::default()
    })
    .await;
    h.exhaust_tenant("met-t", "wait", 2).await;
    let body = h.metrics().await;
    assert!(body.contains("rate_limited_total{policy=\"wait\",tenant=\"met-t\"}"));
}

/// TC-DEP-02 smoke：healthz。
#[tokio::test]
async fn tc_dep_02_healthz() {
    let h = TestHarness::start().await;
    let resp = h.client.get(format!("{}/healthz", h.base_url)).send().await.expect("healthz");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}
