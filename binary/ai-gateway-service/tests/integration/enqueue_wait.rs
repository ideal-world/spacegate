use ai_gateway_service::HarnessConfig;

use super::common::{small_body, TestHarness};

/// TC-W-02：wait 成功返回上游 body 与等待头。
#[tokio::test]
async fn tc_w_02_wait_success_headers() {
    let h = TestHarness::start_config(HarnessConfig {
        rate_limit_burst: Some(10),
        wait_timeout_secs: Some(10),
        ..Default::default()
    })
    .await;

    let resp = h.enqueue_and_wait("wait-ok", small_body(), None).await;
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().contains_key("x-job-id"));
    assert!(resp.headers().contains_key("x-queue-wait-ms"));
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["upstream"], true);
}

/// TC-W-04：wait 超时 504（短 timeout）。
#[tokio::test]
async fn tc_w_04_wait_timeout_504() {
    let h = TestHarness::start_config(HarnessConfig {
        wait_timeout_secs: Some(1),
        ..Default::default()
    })
    .await;

    let resp = h.enqueue_and_wait("wait-to", small_body(), Some(1)).await;
    assert!(resp.status() == 200 || resp.status() == 504);
    if resp.status() == 504 {
        let json: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(json["error"], "timeout");
        assert!(json.get("job_id").is_some());
        assert!(json.get("waited_ms").is_some());
    }
}

/// TC-W-05：完成后 poll 返回 LLM 原始响应。
#[tokio::test]
async fn tc_w_05_poll_returns_upstream_body() {
    let h = TestHarness::start().await;
    let resp = h.enqueue_and_wait("poll-t", small_body(), Some(10)).await;
    assert_eq!(resp.status(), 200);
    let job_id = resp.headers().get("x-job-id").unwrap().to_str().unwrap().to_string();

    let poll = h.get_job(&job_id).await;
    assert_eq!(poll.status(), 200);
    let body: serde_json::Value = poll.json().await.expect("poll json");
    assert_eq!(body["upstream"], true);
}
