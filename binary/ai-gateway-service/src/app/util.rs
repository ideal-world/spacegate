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
    let now = now_ms();
    let seq = JOB_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{now:x}{seq:x}")
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
