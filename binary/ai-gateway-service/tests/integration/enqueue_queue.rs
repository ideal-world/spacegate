use axum::http::HeaderMap;

use ai_gateway_service::HarnessConfig;

use super::common::{small_body, TestHarness};

/// TC-HDR-04：queue 缺 callback。
#[tokio::test]
async fn tc_hdr_04_queue_missing_callback() {
    let h = TestHarness::start().await;
    let resp = h
        .client
        .post(format!("{}/v1/queue/enqueue", h.base_url))
        .header("x-tenant-id", "t1")
        .header("x-ratelimit-policy", "queue")
        .body(small_body())
        .send()
        .await
        .expect("enqueue");
    assert_eq!(resp.status(), 400);
}

/// TC-HDR-05：非 HTTPS 回调（生产配置）。
#[tokio::test]
async fn tc_hdr_05_https_callback_required() {
    let h = TestHarness::start_config(HarnessConfig {
        require_https_callback: Some(true),
        ..Default::default()
    })
    .await;
    let resp = h
        .client
        .post(format!("{}/v1/queue/enqueue", h.base_url))
        .header("x-tenant-id", "t1")
        .header("x-ratelimit-policy", "queue")
        .header("x-callback-url", "http://insecure.example/cb")
        .body(small_body())
        .send()
        .await
        .expect("enqueue");
    assert_eq!(resp.status(), 400);
}

/// TC-Q-02 / TC-Q-03：入队 202 + ULID job_id + poll_url。
#[tokio::test]
async fn tc_q_02_enqueue_returns_202_with_job_id() {
    let h = TestHarness::start().await;

    let resp = h.enqueue("queue-t", small_body(), HeaderMap::new()).await;
    assert_eq!(resp.status(), 202);
    let job_id = resp.headers().get("x-job-id").unwrap().to_str().unwrap().to_string();
    assert_eq!(job_id.len(), 26);
    let json: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(json["status"], "queued");
    assert!(json["poll_url"].as_str().unwrap().contains(&job_id));
}

/// TC-Q-04 / TC-Q-05：Worker 回调四字段 JSON。
#[tokio::test]
async fn tc_q_04_callback_payload_shape() {
    let h = TestHarness::start_config(HarnessConfig {
        rate_limit_burst: Some(10),
        rate_limit_rps: Some(100),
        ..Default::default()
    })
    .await;

    let resp = h.enqueue("cb-t", small_body(), HeaderMap::new()).await;
    assert_eq!(resp.status(), 202);
    let job_id = resp.headers().get("x-job-id").unwrap().to_str().unwrap().to_string();

    for _ in 0..40 {
        if !h.callback_records().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let records = h.callback_records();
    assert!(!records.is_empty(), "expected callback");
    let rec = records.iter().find(|r| r.job_id == job_id).expect("job callback");
    assert!(rec.body.get("job_id").is_some());
    assert!(rec.body.get("status").is_some());
    assert!(rec.body.get("result").is_some());
    assert!(rec.body.get("completed_at").is_some());
    assert!(rec.body.get("http_status").is_none());
}

/// TC-Q-07：dev 允许 http 回调。
#[tokio::test]
async fn tc_q_07_http_callback_when_disabled_check() {
    let h = TestHarness::start_config(HarnessConfig {
        require_https_callback: Some(false),
        ..Default::default()
    })
    .await;
    let resp = h
        .client
        .post(format!("{}/v1/queue/enqueue", h.base_url))
        .header("x-tenant-id", "t-http")
        .header("x-ratelimit-policy", "queue")
        .header("x-callback-url", "http://127.0.0.1:9/cb")
        .body(small_body())
        .send()
        .await
        .expect("enqueue");
    assert_eq!(resp.status(), 202);
}
