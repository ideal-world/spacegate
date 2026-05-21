async fn tenant_rate_limit(state: &AppState, tenant: &str, model: &str, path: &str, policy: &str) -> Result<TenantRateLimit, ServiceError> {
    for key in tenant_rate_limit_candidate_keys(state, tenant, model, path, policy) {
        let raw: Option<String> = state.redis.get(key.as_str()).await.unwrap_or(None);
        if let Some(limit) = raw.and_then(|raw| parse_tenant_rate_limit(&raw)) {
            return Ok(limit);
        }
    }

    let key = format!("{}{}", state.cfg.tenant_rate_limit_prefix, sanitize_key(tenant));
    let rps: Option<String> = state.redis.get(format!("{key}:rps")).await.unwrap_or(None);
    let burst: Option<String> = state.redis.get(format!("{key}:burst")).await.unwrap_or(None);
    let cost: Option<String> = state.redis.get(format!("{key}:cost")).await.unwrap_or(None);
    Ok(TenantRateLimit {
        rps: rps.and_then(|v| v.parse().ok()).unwrap_or(state.cfg.rate_limit_rps),
        burst: burst.and_then(|v| v.parse().ok()).unwrap_or(state.cfg.rate_limit_burst),
        cost: cost.and_then(|v| v.parse().ok()).unwrap_or(state.cfg.rate_limit_cost).max(1),
    })
}

fn tenant_rate_limit_candidate_keys(state: &AppState, tenant: &str, model: &str, path: &str, policy: &str) -> Vec<String> {
    let base = format!("{}{}", state.cfg.tenant_rate_limit_prefix, sanitize_key(tenant));
    let model = sanitize_key(model);
    let path = sanitize_key(path);
    let policy = sanitize_key(policy);
    vec![
        format!("{base}:model:{model}:path:{path}:policy:{policy}"),
        format!("{base}:model:{model}:path:{path}"),
        format!("{base}:model:{model}:policy:{policy}"),
        format!("{base}:path:{path}:policy:{policy}"),
        format!("{base}:model:{model}"),
        format!("{base}:path:{path}"),
        format!("{base}:policy:{policy}"),
        base,
    ]
}

fn parse_tenant_rate_limit(raw: &str) -> Option<TenantRateLimit> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(mut limit) = serde_json::from_str::<TenantRateLimit>(raw) {
        limit.cost = limit.cost.max(1);
        return Some(limit);
    }

    let mut parts = raw.split(',').map(str::trim);
    let rps = parts.next()?.parse().ok()?;
    let burst = parts.next()?.parse().ok()?;
    let cost = parts.next().and_then(|value| value.parse().ok()).unwrap_or(1);
    Some(TenantRateLimit { rps, burst, cost: cost.max(1) })
}
