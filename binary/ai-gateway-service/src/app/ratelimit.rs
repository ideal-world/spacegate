fn global_rate_limit(state: &AppState) -> TenantRateLimit {
    TenantRateLimit {
        rps: state.cfg.rate_limit_rps,
        burst: state.cfg.rate_limit_burst,
        cost: state.cfg.rate_limit_cost.max(1),
    }
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

struct ParsedStoredTenantRateLimit {
    limit: TenantRateLimit,
    ttl_secs: Option<u64>,
}

fn parse_stored_tenant_rate_limit(raw: &str) -> Option<ParsedStoredTenantRateLimit> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(stored) = serde_json::from_str::<StoredTenantRateLimit>(raw) {
        return Some(ParsedStoredTenantRateLimit {
            limit: TenantRateLimit {
                rps: stored.rps,
                burst: stored.burst,
                cost: stored.cost.max(1),
            },
            ttl_secs: stored.ttl_secs,
        });
    }
    if let Ok(mut limit) = serde_json::from_str::<TenantRateLimit>(raw) {
        limit.cost = limit.cost.max(1);
        return Some(ParsedStoredTenantRateLimit { limit, ttl_secs: None });
    }
    parse_tenant_rate_limit_csv(raw).map(|limit| ParsedStoredTenantRateLimit { limit, ttl_secs: None })
}

#[cfg(test)]
fn parse_tenant_rate_limit(raw: &str) -> Option<TenantRateLimit> {
    parse_stored_tenant_rate_limit(raw).map(|stored| stored.limit)
}

fn parse_tenant_rate_limit_csv(raw: &str) -> Option<TenantRateLimit> {
    let mut parts = raw.split(',').map(str::trim);
    let rps = parts.next()?.parse().ok()?;
    let burst = parts.next()?.parse().ok()?;
    let cost = parts.next().and_then(|value| value.parse().ok()).unwrap_or(1);
    Some(TenantRateLimit { rps, burst, cost: cost.max(1) })
}

/// 按租户规则（可含 model/path/policy 维度）解析配额，未命中则回退全局默认值。
async fn resolve_rate_limit(state: &AppState, tenant: &str, model: &str, path: &str, policy: &str) -> Result<TenantRateLimit, ServiceError> {
    for key in tenant_rate_limit_candidate_keys(state, tenant, model, path, policy) {
        let raw: Option<String> = state.redis.get(key.as_str()).await?;
        if let Some(stored) = raw.and_then(|raw| parse_stored_tenant_rate_limit(&raw)) {
            return Ok(stored.limit);
        }
    }
    Ok(global_rate_limit(state))
}

fn tenant_rate_limit_keys(tenant: &str) -> (String, String) {
    let tenant_key = sanitize_key(tenant);
    (
        format!("ai:ratelimit:{tenant_key}:tokens"),
        format!("ai:ratelimit:{tenant_key}:ts"),
    )
}

async fn list_tenant_rate_limit_rules(state: &AppState, filters: &HashMap<String, String>) -> Result<Vec<TenantRateLimitRuleView>, ServiceError> {
    let pattern = format!("{}*", state.cfg.tenant_rate_limit_prefix);
    let mut stream = state.redis.scan_buffered(pattern, Some(100), None);
    let mut out = Vec::new();

    while let Some(key) = stream.next().await {
        let key = key?.into_string().unwrap_or_default();
        if is_legacy_tenant_rate_limit_key(&key) {
            continue;
        }

        let raw: Option<String> = state.redis.get(key.as_str()).await?;
        let Some(stored) = raw.and_then(|raw| parse_stored_tenant_rate_limit(&raw)) else {
            continue;
        };
        let Some(mut rule) = tenant_rate_limit_rule_from_key(state, &key, stored.limit, stored.ttl_secs) else {
            continue;
        };
        rule.cost = rule.cost.max(1);
        if tenant_rule_matches_filters(&rule, filters) {
            let ttl_remaining_secs = read_ttl_remaining_secs(state, key.as_str()).await;
            out.push(tenant_rate_limit_rule_view(key, rule, ttl_remaining_secs));
        }
    }

    out.sort_by(|a, b| tenant_rule_specificity_rule(a).cmp(&tenant_rule_specificity_rule(b)).then_with(|| a.key.cmp(&b.key)));
    Ok(out)
}

async fn upsert_tenant_rate_limit_rule(state: &AppState, mut rule: TenantRateLimitRule) -> Result<TenantRateLimitRuleView, ServiceError> {
    validate_tenant_rate_limit_rule(&rule)?;
    rule.cost = rule.cost.max(1);
    let key = tenant_rate_limit_rule_key(state, &rule);
    let value = serde_json::to_string(&StoredTenantRateLimit {
        rps: rule.rps,
        burst: rule.burst,
        cost: rule.cost,
        ttl_secs: rule.ttl_secs,
    })
    .map_err(|e| ServiceError::internal(format!("serialize tenant rate limit: {e}")))?;
    let expiration = rule.ttl_secs.map(|ttl| Expiration::EX(ttl.max(1) as i64));
    let _: String = state.redis.set(key.as_str(), value, expiration, None, false).await?;
    let ttl_remaining_secs = read_ttl_remaining_secs(state, key.as_str()).await;
    Ok(tenant_rate_limit_rule_view(key, rule, ttl_remaining_secs))
}

async fn delete_tenant_rate_limit_rule(state: &AppState, rule: TenantRateLimitRule) -> Result<u64, ServiceError> {
    validate_tenant_rule_dimensions(&rule)?;
    let key = tenant_rate_limit_rule_key(state, &rule);
    let removed: u64 = state.redis.del(key.as_str()).await?;
    Ok(removed)
}

async fn read_ttl_remaining_secs(state: &AppState, key: &str) -> Option<i64> {
    let ttl: i64 = state.redis.ttl(key).await.unwrap_or(-2);
    if ttl > 0 { Some(ttl) } else { None }
}

fn tenant_rate_limit_rule_key(state: &AppState, rule: &TenantRateLimitRule) -> String {
    let base = format!("{}{}", state.cfg.tenant_rate_limit_prefix, sanitize_key(rule.tenant.trim()));
    let mut key = base;
    if let Some(model) = non_empty_opt(&rule.model) {
        key.push_str(":model:");
        key.push_str(&sanitize_key(model));
    }
    if let Some(path) = non_empty_opt(&rule.path) {
        key.push_str(":path:");
        key.push_str(&sanitize_key(path));
    }
    if let Some(policy) = non_empty_opt(&rule.policy) {
        key.push_str(":policy:");
        key.push_str(&sanitize_key(policy));
    }
    key
}

fn tenant_rate_limit_rule_from_key(state: &AppState, key: &str, limit: TenantRateLimit, ttl_secs: Option<u64>) -> Option<TenantRateLimitRule> {
    let rest = key.strip_prefix(&state.cfg.tenant_rate_limit_prefix)?;
    let mut parts = rest.split(':');
    let tenant = parts.next()?.to_string();
    if tenant.is_empty() {
        return None;
    }

    let mut model = None;
    let mut path = None;
    let mut policy = None;
    while let (Some(name), Some(value)) = (parts.next(), parts.next()) {
        match name {
            "model" => model = Some(value.to_string()),
            "path" => path = Some(value.to_string()),
            "policy" => policy = Some(value.to_string()),
            _ => {}
        }
    }

    Some(TenantRateLimitRule {
        tenant,
        model,
        path,
        policy,
        rps: limit.rps,
        burst: limit.burst,
        cost: limit.cost.max(1),
        ttl_secs,
    })
}

fn validate_tenant_rate_limit_rule(rule: &TenantRateLimitRule) -> Result<(), ServiceError> {
    validate_tenant_rule_dimensions(rule)?;
    if rule.rps == 0 {
        return Err(ServiceError::bad_request("rps must be greater than 0"));
    }
    if rule.burst == 0 {
        return Err(ServiceError::bad_request("burst must be greater than 0"));
    }
    if rule.cost == 0 {
        return Err(ServiceError::bad_request("cost must be greater than 0"));
    }
    Ok(())
}

fn validate_tenant_rule_dimensions(rule: &TenantRateLimitRule) -> Result<(), ServiceError> {
    if rule.tenant.trim().is_empty() {
        return Err(ServiceError::bad_request("tenant is required"));
    }
    if let Some(policy) = non_empty_opt(&rule.policy) {
        match policy {
            "abandon" | "queue" | "wait" => {}
            _ => return Err(ServiceError::bad_request("policy must be abandon, queue, or wait")),
        }
    }
    Ok(())
}

fn tenant_rule_matches_filters(rule: &TenantRateLimitRule, filters: &HashMap<String, String>) -> bool {
    for (name, value) in filters {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let matches = match name.as_str() {
            "tenant" => rule.tenant.contains(value),
            "model" => rule.model.as_deref().unwrap_or("").contains(value),
            "path" => rule.path.as_deref().unwrap_or("").contains(value),
            "policy" => rule.policy.as_deref().unwrap_or("") == value,
            _ => true,
        };
        if !matches {
            return false;
        }
    }
    true
}

fn tenant_rule_specificity_rule(view: &TenantRateLimitRuleView) -> usize {
    usize::from(non_empty_opt(&view.model).is_some()) + usize::from(non_empty_opt(&view.path).is_some()) + usize::from(non_empty_opt(&view.policy).is_some())
}

fn is_legacy_tenant_rate_limit_key(key: &str) -> bool {
    key.ends_with(":rps") || key.ends_with(":burst") || key.ends_with(":cost")
}

fn non_empty_opt(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|value| !value.is_empty())
}

fn record_rate_limited(metrics: &Metrics, policy: &str, tenant: &str) {
    metrics.rate_limited_total.fetch_add(1, Ordering::Relaxed);
    inc_labeled(
        metrics,
        format!(
            r#"rate_limited_total{{policy="{}",tenant="{}"}}"#,
            metrics_label(policy),
            metrics_label(tenant)
        ),
    );
}
