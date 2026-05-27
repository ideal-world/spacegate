use ai_gateway_service::HarnessConfig;

use super::common::{parse_rate_limit, TestHarness};

/// TC-RL-01 / TC-RL-02：租户隔离与配额内 allowed。
#[tokio::test]
async fn tc_rl_01_tenant_isolation_and_allowed() {
    let h = TestHarness::start_config(HarnessConfig {
        rate_limit_rps: Some(1),
        rate_limit_burst: Some(1),
        ..Default::default()
    })
    .await;

    let a1 = parse_rate_limit(h.check_rate_limit("tenant-a", "abandon").await).await;
    assert_eq!(a1["allowed"], true);

    let a2 = parse_rate_limit(h.check_rate_limit("tenant-a", "abandon").await).await;
    assert_eq!(a2["allowed"], false);

    let b1 = parse_rate_limit(h.check_rate_limit("tenant-b", "abandon").await).await;
    assert_eq!(b1["allowed"], true);
}

/// TC-RL-03：超额时 metrics 计数。
#[tokio::test]
async fn tc_rl_03_rate_limited_metrics() {
    let h = TestHarness::start_config(HarnessConfig {
        rate_limit_burst: Some(1),
        rate_limit_rps: Some(1),
        ..Default::default()
    })
    .await;

    h.exhaust_tenant("metrics-tenant", "queue", 2).await;
    let body = h.metrics().await;
    assert!(body.contains("rate_limited_total"));
    assert!(body.contains("policy=\"queue\""));
    assert!(body.contains("tenant=\"metrics-tenant\""));
}

/// TC-RL-04：burst 超发后第三次拒绝。
#[tokio::test]
async fn tc_rl_04_burst_then_deny() {
    let h = TestHarness::start_config(HarnessConfig {
        rate_limit_burst: Some(2),
        rate_limit_rps: Some(100),
        ..Default::default()
    })
    .await;

    for _ in 0..2 {
        let v = parse_rate_limit(h.check_rate_limit("burst-t", "abandon").await).await;
        assert_eq!(v["allowed"], true);
    }
    let v = parse_rate_limit(h.check_rate_limit("burst-t", "abandon").await).await;
    assert_eq!(v["allowed"], false);
    assert!(v["retry_after_ms"].as_i64().unwrap_or(0) > 0);
}

/// TC-RL-07：Redis 限流 key 仅 tenant 维度。
#[tokio::test]
async fn tc_rl_07_tenant_only_redis_keys() {
    let h = TestHarness::start().await;
    let _ = h.check_rate_limit("key-tenant", "abandon").await;
    let keys = h.ratelimit_keys_for_tenant("key-tenant").await;
    assert!(keys.iter().any(|k| k.ends_with(":tokens")));
    assert!(keys.iter().any(|k| k.ends_with(":ts")));
    assert!(!keys.iter().any(|k| k.contains("model") || k.contains("path")));
}
