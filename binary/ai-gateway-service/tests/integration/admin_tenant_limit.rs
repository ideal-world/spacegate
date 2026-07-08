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
    let put = h.client.put(format!("{}/v1/admin/tenant-rate-limits", h.base_url)).json(&rule).send().await.expect("put rule");
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
    h.client.put(format!("{}/v1/admin/tenant-rate-limits", h.base_url)).json(&rule).send().await.expect("put");

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

#[tokio::test]
async fn tc_rl_08_admin_rejects_unusable_rule_dimensions() {
    let h = TestHarness::start().await;

    let cost_above_burst = serde_json::json!({
        "tenant": "bad-cost",
        "rps": 10,
        "burst": 2,
        "cost": 3
    });
    let resp = h.client.put(format!("{}/v1/admin/tenant-rate-limits", h.base_url)).json(&cost_above_burst).send().await.expect("put");
    assert_eq!(resp.status(), 400);
    let body = resp.text().await.expect("body");
    assert!(body.contains("cost must be less than or equal to burst"));

    let colon_dimension = serde_json::json!({
        "tenant": "tenant:bad",
        "rps": 10,
        "burst": 20,
        "cost": 1
    });
    let resp = h.client.put(format!("{}/v1/admin/tenant-rate-limits", h.base_url)).json(&colon_dimension).send().await.expect("put");
    assert_eq!(resp.status(), 400);
    let body = resp.text().await.expect("body");
    assert!(body.contains("tenant must not contain ':'"));
}

#[tokio::test]
async fn tc_rl_09_admin_lists_specific_rules_first() {
    let h = TestHarness::start().await;
    for rule in [
        serde_json::json!({
            "tenant": "sort-tenant",
            "rps": 100,
            "burst": 100,
            "cost": 1
        }),
        serde_json::json!({
            "tenant": "sort-tenant",
            "model": "gpt-4",
            "path": "/v1/chat/completions",
            "policy": "wait",
            "rps": 1,
            "burst": 1,
            "cost": 1
        }),
    ] {
        let resp = h.client.put(format!("{}/v1/admin/tenant-rate-limits", h.base_url)).json(&rule).send().await.expect("put");
        assert_eq!(resp.status(), 200);
    }

    let list = h.client.get(format!("{}/v1/admin/tenant-rate-limits?tenant=sort-tenant", h.base_url)).send().await.expect("list").json::<serde_json::Value>().await.expect("json");
    let rows = list.as_array().expect("rows");
    assert!(rows.len() >= 2);
    assert_eq!(rows[0]["model"], "gpt-4");
    assert_eq!(rows[0]["policy"], "wait");
    assert!(rows[0]["key"].as_str().unwrap_or_default().contains(":model:gpt-4:path:_v1_chat_completions:policy:wait"));
}

#[tokio::test]
async fn tc_rl_10_admin_resolve_explains_matched_rule() {
    let h = TestHarness::start().await;
    let specific = serde_json::json!({
        "tenant": "resolve-tenant",
        "model": "gpt-4",
        "path": "/v1/chat/completions",
        "policy": "wait",
        "rps": 1,
        "burst": 2,
        "cost": 2
    });
    let resp = h.client.put(format!("{}/v1/admin/tenant-rate-limits", h.base_url)).json(&specific).send().await.expect("put");
    assert_eq!(resp.status(), 200);

    let resolved = h
        .client
        .get(format!(
            "{}/v1/admin/tenant-rate-limits/resolve?tenant=resolve-tenant&model=gpt-4&path=/v1/chat/completions&policy=wait",
            h.base_url
        ))
        .send()
        .await
        .expect("resolve")
        .json::<serde_json::Value>()
        .await
        .expect("json");

    assert_eq!(resolved["tenant"], "resolve-tenant");
    assert_eq!(resolved["rps"], 1);
    assert_eq!(resolved["burst"], 2);
    assert_eq!(resolved["cost"], 2);
    assert_eq!(resolved["fallback_global"], false);
    assert!(resolved["matched_key"].as_str().unwrap_or_default().contains(":model:gpt-4:path:_v1_chat_completions:policy:wait"));
    assert_eq!(resolved["candidate_keys"].as_array().expect("candidates").len(), 8);
}
