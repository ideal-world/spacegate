use ai_gateway_service::HarnessConfig;

use super::common::{small_body, TestHarness};

/// TC-BODY-01：小 body inline 入队。
#[tokio::test]
async fn tc_body_01_inline_enqueue() {
    let h = TestHarness::start().await;
    let resp = h.enqueue("inline-t", small_body(), axum::http::HeaderMap::new()).await;
    assert_eq!(resp.status(), 202);
}

/// TC-BODY-03：无 S3 时大 body 413。
#[tokio::test]
async fn tc_body_03_large_body_without_s3_rejected() {
    let h = TestHarness::start_config(HarnessConfig {
        inline_threshold: Some(1024),
        clear_object_store: true,
        ..Default::default()
    })
    .await;
    let large = vec![0u8; 2048];
    let resp = h.enqueue("large-t", large, axum::http::HeaderMap::new()).await;
    assert_eq!(resp.status(), 413);
}

/// TC-HDR-03：缺 tenant。
#[tokio::test]
async fn tc_hdr_03_missing_tenant() {
    let h = TestHarness::start().await;
    let resp = h.client.post(format!("{}/v1/ratelimit/check", h.base_url)).header("x-ratelimit-policy", "abandon").send().await.expect("check");
    assert_eq!(resp.status(), 400);
}
