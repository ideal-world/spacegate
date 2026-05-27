fn required_header(headers: &HeaderMap, name: &str) -> Result<String, ServiceError> {
    optional_header(headers, name).ok_or_else(|| ServiceError::bad_request(format!("missing required header `{name}`")))
}

fn optional_header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name).and_then(|value| value.to_str().ok()).map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned)
}

fn headers_to_json(headers: &HeaderMap) -> Result<String, ServiceError> {
    let mut out = HashMap::new();
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            out.insert(name.as_str().to_string(), value.to_string());
        }
    }
    serde_json::to_string(&out).map_err(|e| ServiceError::internal(format!("serialize headers: {e}")))
}

fn should_forward_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !matches!(
        name.as_str(),
        "host" | "connection" | "content-length" | "transfer-encoding" | "x-original-method" | "x-original-path" | "x-ratelimit-policy" | "x-callback-url" | "x-request-timeout"
    )
}

fn header_value(value: &str) -> Result<HeaderValue, ServiceError> {
    HeaderValue::from_str(value).map_err(|e| ServiceError::internal(format!("invalid response header value: {e}")))
}

fn result_key(state: &AppState, job_id: &str) -> String {
    format!("{}{}", state.cfg.result_key_prefix, job_id)
}

fn result_channel(state: &AppState, job_id: &str) -> String {
    format!("{}{}", state.cfg.result_channel_prefix, job_id)
}

fn new_job_id() -> String {
    ulid::Ulid::new().to_string()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn sanitize_key(input: &str) -> String {
    input.chars().map(|ch| if ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '-' | '.') { ch } else { '_' }).collect()
}

fn build_redis_client(url: &str) -> Result<FredClient, fred::error::Error> {
    let config = Config::from_url(url)?;
    Builder::from_config(config).build()
}

fn build_subscriber_client(url: &str) -> Result<SubscriberClient, fred::error::Error> {
    let config = Config::from_url(url)?;
    Builder::from_config(config).build_subscriber_client()
}

fn field_string(fields: &HashMap<String, Value>, key: &str) -> Option<String> {
    fields.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.to_string()),
        Value::Bytes(value) => String::from_utf8(value.to_vec()).ok(),
        Value::Integer(value) => Some(value.to_string()),
        _ => None,
    })
}

fn field_bytes(fields: &HashMap<String, Value>, key: &str) -> Option<Vec<u8>> {
    fields.get(key).and_then(|value| match value {
        Value::Bytes(value) => Some(value.to_vec()),
        Value::String(value) => Some(value.as_bytes().to_vec()),
        _ => None,
    })
}

fn field_u64(fields: &HashMap<String, Value>, key: &str) -> Option<u64> {
    fields.get(key).and_then(|value| match value {
        Value::Integer(value) => (*value).try_into().ok(),
        Value::String(value) => value.parse().ok(),
        Value::Bytes(value) => std::str::from_utf8(value).ok().and_then(|value| value.parse().ok()),
        _ => None,
    })
}

fn field_u32(fields: &HashMap<String, Value>, key: &str) -> Option<u32> {
    field_u64(fields, key).and_then(|value| value.try_into().ok())
}

fn job_poll_url(job_id: &str) -> String {
    format!("/jobs/{job_id}/status")
}

fn job_status_url_legacy(job_id: &str) -> String {
    format!("/v1/jobs/{job_id}")
}

fn metrics_label(value: &str) -> String {
    sanitize_key(value).chars().take(64).collect()
}

fn body_size_bucket(size: usize, storage: &str) -> &'static str {
    if storage == "object" || storage == "s3" {
        "s3"
    } else if size <= 10 * 1024 {
        "inline_small"
    } else if size <= 128 * 1024 {
        "inline"
    } else {
        "inline_large"
    }
}

fn format_completed_at_rfc3339(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let rem_ms = ms % 86_400_000;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem_ms / 3_600_000,
        (rem_ms % 3_600_000) / 60_000,
        (rem_ms % 60_000) / 1_000,
        rem_ms % 1_000,
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y, m as u32, d as u32)
}

fn decode_callback_result(body_base64: &str) -> serde_json::Value {
    if body_base64.is_empty() {
        return serde_json::Value::Null;
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(body_base64) else {
        return serde_json::json!({ "raw_base64": body_base64 });
    };
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        return value;
    }
    serde_json::json!({ "raw_base64": body_base64 })
}

fn poll_result_to_response(result: StoredResult) -> Result<Response, ServiceError> {
    let status = StatusCode::from_u16(result.http_status).unwrap_or(StatusCode::OK);
    let body = base64::engine::general_purpose::STANDARD.decode(result.body_base64).map_err(|e| ServiceError::internal(format!("decode poll result body: {e}")))?;
    let mut resp = (status, body).into_response();
    for (name, value) in result.headers {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(name.as_str()), HeaderValue::from_str(&value)) {
            resp.headers_mut().insert(name, value);
        }
    }
    resp.headers_mut().insert("x-job-id", header_value(&result.job_id)?);
    Ok(resp)
}

fn tenant_rate_limit_rule_view(key: String, rule: TenantRateLimitRule, ttl_remaining_secs: Option<i64>) -> TenantRateLimitRuleView {
    TenantRateLimitRuleView {
        key,
        tenant: rule.tenant,
        model: rule.model,
        path: rule.path,
        policy: rule.policy,
        rps: rule.rps,
        burst: rule.burst,
        cost: rule.cost,
        ttl_secs: rule.ttl_secs,
        ttl_remaining_secs,
    }
}

/// Parse `XREADGROUP` into the map form used by workers.
///
/// Redis returns `nil` when a blocking read times out or the stream has no new entries for `>`.
/// Without explicit handling, fred fails to convert that into `HashMap` and the worker aborts
/// before polling the next priority stream — even when another stream already has backlog.
async fn xreadgroup_map_or_empty(
    redis: &FredClient,
    group: &str,
    consumer: &str,
    count: Option<u64>,
    block: Option<u64>,
    noack: bool,
    keys: Vec<&str>,
    ids: Vec<&str>,
) -> Result<XReadResponse<String, String, String, Value>, ServiceError> {
    let value: Value = redis.xreadgroup(group, consumer, count, block, noack, keys, ids).await?;
    if value.is_null() {
        return Ok(HashMap::new());
    }
    value
        .into_xread_response()
        .map_err(|e| ServiceError::internal(format!("parse xreadgroup response: {e}")))
}
