use ai_gateway_service::HarnessConfig;

use super::common::TestHarness;

/// TC-RL-05 / TC-RL-06：Admin 租户规则写入并生效。
#[tokio::test]
async fn tc_rl_05_admin_tenant_rate_limit() {
    let h = TestHarness::start_config(HarnessConfig {
        rate_limit_rps: Some(100),
        rate_limit_burst: Some(100),
        ..Default::default()
    })
    .await;

    let rule = serde_json::json!({
        "tenant": "admin-tenant",
        "rps": 1,
        "burst": 1,
        "cost": 1
    });
    let put = h
        .client
        .put(format!("{}/v1/admin/tenant-rate-limits", h.base_url))
        .json(&rule)
        .send()
        .await
        .expect("put rule");
    assert_eq!(put.status(), 200);

    let first = h.check_rate_limit("admin-tenant", "abandon").await.json::<serde_json::Value>().await.unwrap();
    assert_eq!(first["allowed"], true);
    let second = h.check_rate_limit("admin-tenant", "abandon").await.json::<serde_json::Value>().await.unwrap();
    assert_eq!(second["allowed"], false);
}

/// TC-RL-06：model 维度规则更具体时生效。
#[tokio::test]
async fn tc_rl_06_model_specific_rule() {
    let h = TestHarness::start().await;
    let rule = serde_json::json!({
        "tenant": "model-tenant",
        "model": "gpt-4",
        "rps": 1,
        "burst": 1
    });
    h.client
        .put(format!("{}/v1/admin/tenant-rate-limits", h.base_url))
        .json(&rule)
        .send()
        .await
        .expect("put");

    let resp = h
        .client
        .post(format!("{}/v1/ratelimit/check", h.base_url))
        .header("x-tenant-id", "model-tenant")
        .header("x-model", "gpt-4")
        .header("x-ratelimit-policy", "abandon")
        .send()
        .await
        .expect("check");
    let first = resp.json::<serde_json::Value>().await.unwrap();
    assert_eq!(first["allowed"], true);
}
