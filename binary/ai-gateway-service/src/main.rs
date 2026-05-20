use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use clap::Parser;
use fred::clients::{Client as FredClient, SubscriberClient};
use fred::prelude::*;
use fred::types::streams::XReadResponse;
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

const TOKEN_BUCKET_LUA: &str = r#"
local tokens_key = KEYS[1]
local ts_key = KEYS[2]
local rate = tonumber(ARGV[1])
local burst = tonumber(ARGV[2])
local now = tonumber(ARGV[3])
local cost = tonumber(ARGV[4])

if rate <= 0 or burst <= 0 or cost <= 0 then
  return {0, 0, 1000}
end

local burst_milli = burst * 1000
local cost_milli = cost * 1000
local tokens = tonumber(redis.call('GET', tokens_key) or burst_milli)
local last_ts = tonumber(redis.call('GET', ts_key) or now)
local elapsed = math.max(0, now - last_ts)
tokens = math.min(burst_milli, tokens + elapsed * rate)

local ttl = math.max(1000, math.ceil((burst_milli / rate) * 2))
if tokens >= cost_milli then
  tokens = tokens - cost_milli
  redis.call('SET', tokens_key, tokens, 'PX', ttl)
  redis.call('SET', ts_key, now, 'PX', ttl)
  return {1, tokens, 0}
else
  local wait_ms = math.ceil((cost_milli - tokens) / rate)
  redis.call('SET', tokens_key, tokens, 'PX', ttl)
  redis.call('SET', ts_key, now, 'PX', ttl)
  return {0, tokens, wait_ms}
end
"#;

#[derive(Debug, Clone, Parser)]
#[command(version, about = "External Redis-backed rate-limit and queue service for SpaceGate AI gateway")]
struct Args {
    #[arg(long, env = "AI_GATEWAY_SERVICE_HOST", default_value = "0.0.0.0")]
    host: IpAddr,
    #[arg(long, env = "AI_GATEWAY_SERVICE_PORT", default_value_t = 18080)]
    port: u16,
    #[arg(long, env = "REDIS_URL", default_value = "redis://127.0.0.1/")]
    redis_url: String,
    #[arg(long, env = "AI_QUEUE_STREAM", default_value = "ai:jobs")]
    stream_key: String,
    #[arg(long, env = "AI_QUEUE_GROUP", default_value = "ai-gateway-workers")]
    consumer_group: String,
    #[arg(long, env = "AI_QUEUE_CONSUMER", default_value = "ai-gateway-service")]
    consumer_name: String,
    #[arg(long, env = "AI_RESULT_KEY_PREFIX", default_value = "result:")]
    result_key_prefix: String,
    #[arg(long, env = "AI_RESULT_CHANNEL_PREFIX", default_value = "result:")]
    result_channel_prefix: String,
    #[arg(long, env = "AI_RESULT_TTL_SECS", default_value_t = 120)]
    result_ttl_secs: u64,
    #[arg(long, env = "AI_RATE_LIMIT_RPS", default_value_t = 100)]
    rate_limit_rps: u64,
    #[arg(long, env = "AI_RATE_LIMIT_BURST", default_value_t = 200)]
    rate_limit_burst: u64,
    #[arg(long, env = "AI_WAIT_TIMEOUT_SECS", default_value_t = 60)]
    wait_timeout_secs: u64,
    #[arg(long, env = "AI_WORKER_CONCURRENCY", default_value_t = 1)]
    worker_concurrency: usize,
    #[arg(long, env = "AI_UPSTREAM_BASE_URL")]
    upstream_base_url: Option<String>,
    #[arg(long, env = "AI_MAX_BODY_BYTES", default_value_t = 32 * 1024 * 1024)]
    max_body_bytes: usize,
}

#[derive(Clone)]
struct AppState {
    redis: FredClient,
    http: reqwest::Client,
    cfg: Arc<Args>,
}

#[derive(Debug, Serialize)]
struct RateLimitResponse {
    allowed: bool,
    remaining_tokens_milli: i64,
    retry_after_ms: i64,
}

#[derive(Debug, Serialize)]
struct EnqueueResponse {
    job_id: String,
    stream_id: String,
    status: &'static str,
    poll_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct StoredResult {
    job_id: String,
    status: String,
    http_status: u16,
    headers: HashMap<String, String>,
    body_base64: String,
    completed_at_ms: u64,
    error: Option<String>,
}

#[derive(Debug)]
struct ServiceError {
    status: StatusCode,
    message: String,
}

impl ServiceError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn gateway_timeout(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: message.into(),
        }
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.message }));
        (self.status, body).into_response()
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ServiceError {}

impl From<fred::error::Error> for ServiceError {
    fn from(value: fred::error::Error) -> Self {
        Self::internal(format!("redis: {value}"))
    }
}

impl From<reqwest::Error> for ServiceError {
    fn from(value: reqwest::Error) -> Self {
        Self::internal(format!("http: {value}"))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    let args = Args::parse();
    let redis = build_redis_client(&args.redis_url)?;
    let _redis_task = redis.init().await?;
    let state = AppState {
        redis,
        http: reqwest::Client::new(),
        cfg: Arc::new(args.clone()),
    };

    ensure_consumer_group(&state).await?;
    if state.cfg.upstream_base_url.is_some() {
        spawn_workers(state.clone());
    } else {
        tracing::warn!("AI_UPSTREAM_BASE_URL is not set; queue jobs will be stored but no local worker will process them");
    }

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/ratelimit/check", post(check_rate_limit))
        .route("/v1/queue/enqueue", post(enqueue))
        .route("/v1/queue/enqueue-and-wait", post(enqueue_and_wait))
        .route("/v1/jobs/{job_id}", get(get_job))
        .layer(DefaultBodyLimit::max(args.max_body_bytes))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::new(args.host, args.port);
    tracing::info!(%addr, "ai-gateway-service listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn check_rate_limit(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Result<Json<RateLimitResponse>, ServiceError> {
    let tenant = required_header(&headers, "x-tenant-id")?;
    let model = optional_header(&headers, "x-model").unwrap_or_else(|| "default".to_string());
    let path = optional_header(&headers, "x-original-path").unwrap_or_else(|| uri.path().to_string());
    let key = sanitize_key(&format!("{tenant}:{model}:{path}"));
    let tokens_key = format!("ai:ratelimit:{key}:tokens");
    let ts_key = format!("ai:ratelimit:{key}:ts");
    let now = now_ms();

    let out: Vec<i64> = state
        .redis
        .eval(
            TOKEN_BUCKET_LUA,
            vec![tokens_key, ts_key],
            vec![
                state.cfg.rate_limit_rps.to_string(),
                state.cfg.rate_limit_burst.to_string(),
                now.to_string(),
                "1".to_string(),
            ],
        )
        .await?;

    Ok(Json(RateLimitResponse {
        allowed: out.first().copied().unwrap_or(0) == 1,
        remaining_tokens_milli: out.get(1).copied().unwrap_or(0),
        retry_after_ms: out.get(2).copied().unwrap_or(0),
    }))
}

async fn enqueue(State(state): State<AppState>, method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Result<impl IntoResponse, ServiceError> {
    let accepted = enqueue_job(&state, method, uri, headers, body).await?;
    let mut resp = (StatusCode::ACCEPTED, Json(&accepted)).into_response();
    resp.headers_mut().insert("x-job-id", header_value(&accepted.job_id)?);
    Ok(resp)
}

async fn enqueue_and_wait(State(state): State<AppState>, method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Result<Response, ServiceError> {
    let timeout_secs = optional_header(&headers, "x-request-timeout").and_then(|v| v.parse::<u64>().ok()).unwrap_or(state.cfg.wait_timeout_secs);
    let accepted = enqueue_job(&state, method, uri, headers, body).await?;
    let channel = result_channel(&state, &accepted.job_id);
    let subscriber = build_subscriber_client(&state.cfg.redis_url)?;
    let _subscriber_task = subscriber.init().await?;
    subscriber.subscribe(channel.as_str()).await?;

    if let Some(result) = load_result(&state, &accepted.job_id).await? {
        let _ = subscriber.quit().await;
        return Ok(result_to_response(result)?);
    }

    let mut messages = subscriber.message_rx();
    let wait = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        loop {
            let message = messages.recv().await.map_err(|e| ServiceError::internal(format!("pubsub receive: {e}")))?;
            if &*message.channel == channel.as_str() {
                return Ok::<(), ServiceError>(());
            }
        }
    })
    .await;
    match wait {
        Ok(Ok(())) => {
            let _ = subscriber.quit().await;
            if let Some(result) = load_result(&state, &accepted.job_id).await? {
                Ok(result_to_response(result)?)
            } else {
                Err(ServiceError::gateway_timeout(format!(
                    "job {} completed notification received but result is missing",
                    accepted.job_id
                )))
            }
        }
        _ => {
            let _ = subscriber.quit().await;
            let body = Json(serde_json::json!({
                "error": "timeout",
                "job_id": accepted.job_id,
                "message": "Job is still processing. Switch to queue mode with a callback for long tasks."
            }));
            Ok((StatusCode::GATEWAY_TIMEOUT, body).into_response())
        }
    }
}

async fn get_job(State(state): State<AppState>, Path(job_id): Path<String>) -> Result<Response, ServiceError> {
    match load_result(&state, &job_id).await? {
        Some(result) => Ok(Json(result).into_response()),
        None => Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not_found", "job_id": job_id }))).into_response()),
    }
}

async fn enqueue_job(state: &AppState, _method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Result<EnqueueResponse, ServiceError> {
    let job_id = new_job_id();
    let tenant_id = required_header(&headers, "x-tenant-id")?;
    let policy = optional_header(&headers, "x-ratelimit-policy").unwrap_or_else(|| "queue".to_string());
    let model = optional_header(&headers, "x-model").unwrap_or_else(|| "default".to_string());
    let callback_url = optional_header(&headers, "x-callback-url").unwrap_or_default();
    let original_method = optional_header(&headers, "x-original-method").unwrap_or_else(|| "POST".to_string());
    let original_path = optional_header(&headers, "x-original-path").unwrap_or_else(|| uri.path().to_string());
    let request_headers = headers_to_json(&headers)?;
    let created_at = now_ms();

    let stream_id: String = state
        .redis
        .xadd(
            state.cfg.stream_key.as_str(),
            false,
            None::<()>,
            "*",
            vec![
                ("job_id", Value::String(job_id.clone().into())),
                ("tenant_id", Value::String(tenant_id.into())),
                ("policy", Value::String(policy.into())),
                ("model", Value::String(model.into())),
                ("method", Value::String(original_method.into())),
                ("path", Value::String(original_path.into())),
                ("headers", Value::String(request_headers.into())),
                ("body", Value::Bytes(body)),
                ("callback_url", Value::String(callback_url.into())),
                ("created_at", Value::Integer(created_at as i64)),
            ],
        )
        .await?;

    Ok(EnqueueResponse {
        job_id: job_id.clone(),
        stream_id,
        status: "queued",
        poll_url: format!("/v1/jobs/{job_id}"),
    })
}

fn spawn_workers(state: AppState) {
    for idx in 0..state.cfg.worker_concurrency.max(1) {
        let state = state.clone();
        tokio::spawn(async move {
            let consumer = format!("{}-{idx}", state.cfg.consumer_name);
            loop {
                if let Err(e) = worker_once(&state, &consumer).await {
                    tracing::warn!(error = %e.message, "worker loop failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        });
    }
}

async fn worker_once(state: &AppState, consumer: &str) -> Result<(), ServiceError> {
    let reply: XReadResponse<String, String, String, Value> = state
        .redis
        .xreadgroup_map(
            state.cfg.consumer_group.as_str(),
            consumer,
            Some(5),
            Some(1000),
            false,
            vec![state.cfg.stream_key.as_str()],
            vec![">"],
        )
        .await?;

    for (_stream, entries) in reply {
        for (entry_id, fields) in entries {
            match process_job(state, entry_id.as_str(), &fields).await {
                Ok(()) => {
                    let _: i64 = state.redis.xack(state.cfg.stream_key.as_str(), state.cfg.consumer_group.as_str(), vec![entry_id.clone()]).await?;
                }
                Err(e) => {
                    tracing::warn!(stream_id = %entry_id, error = %e.message, "job processing failed");
                }
            }
        }
    }
    Ok(())
}

async fn process_job(state: &AppState, _stream_id: &str, fields: &HashMap<String, Value>) -> Result<(), ServiceError> {
    let Some(base) = state.cfg.upstream_base_url.as_deref() else {
        return Err(ServiceError::internal("upstream base URL is not configured"));
    };
    let job_id = field_string(fields, "job_id").ok_or_else(|| ServiceError::bad_request("job missing job_id"))?;
    let method = field_string(fields, "method").unwrap_or_else(|| "POST".to_string());
    let path = field_string(fields, "path").unwrap_or_else(|| "/".to_string());
    let headers_json = field_string(fields, "headers").unwrap_or_else(|| "{}".to_string());
    let callback_url = field_string(fields, "callback_url").unwrap_or_default();
    let body = field_bytes(fields, "body").unwrap_or_default();
    let headers: HashMap<String, String> = serde_json::from_str(&headers_json).unwrap_or_default();

    let url = format!("{}{}", base.trim_end_matches('/'), path);
    let parsed_method = method.parse::<reqwest::Method>().unwrap_or(reqwest::Method::POST);
    let mut req = state.http.request(parsed_method, url);
    for (name, value) in headers {
        if should_forward_header(&name) {
            req = req.header(name, value);
        }
    }
    let upstream = req.body(body).send().await;
    let result = match upstream {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mut headers = HashMap::new();
            for (name, value) in resp.headers() {
                if let Ok(value) = value.to_str() {
                    headers.insert(name.as_str().to_string(), value.to_string());
                }
            }
            let body = resp.bytes().await.unwrap_or_default();
            StoredResult {
                job_id: job_id.clone(),
                status: "completed".to_string(),
                http_status: status,
                headers,
                body_base64: base64::engine::general_purpose::STANDARD.encode(body),
                completed_at_ms: now_ms(),
                error: None,
            }
        }
        Err(e) => StoredResult {
            job_id: job_id.clone(),
            status: "failed".to_string(),
            http_status: 502,
            headers: HashMap::new(),
            body_base64: String::new(),
            completed_at_ms: now_ms(),
            error: Some(e.to_string()),
        },
    };

    store_result(state, &result).await?;
    if !callback_url.is_empty() {
        let callback_body = serde_json::json!({
            "job_id": result.job_id,
            "status": result.status,
            "http_status": result.http_status,
            "headers": result.headers,
            "body_base64": result.body_base64,
            "completed_at_ms": result.completed_at_ms,
            "error": result.error,
        });
        if let Err(e) = state.http.post(callback_url).json(&callback_body).send().await {
            tracing::warn!(job_id = %job_id, error = %e, "callback failed");
        }
    }
    Ok(())
}

async fn ensure_consumer_group(state: &AppState) -> Result<(), ServiceError> {
    let res: FredResult<String> = state.redis.xgroup_create(state.cfg.stream_key.as_str(), state.cfg.consumer_group.as_str(), "$", true).await;
    match res {
        Ok(_) => Ok(()),
        Err(e) if e.to_string().contains("BUSYGROUP") => Ok(()),
        Err(e) => Err(e.into()),
    }
}

async fn store_result(state: &AppState, result: &StoredResult) -> Result<(), ServiceError> {
    let json = serde_json::to_string(result).map_err(|e| ServiceError::internal(format!("serialize result: {e}")))?;
    let key = result_key(state, &result.job_id);
    let channel = result_channel(state, &result.job_id);
    let ttl = state.cfg.result_ttl_secs.min(i64::MAX as u64) as i64;
    let _: () = state.redis.set(key, json, Some(Expiration::EX(ttl)), None::<SetOptions>, false).await?;
    let _: i64 = state.redis.publish(channel, "done").await?;
    Ok(())
}

async fn load_result(state: &AppState, job_id: &str) -> Result<Option<StoredResult>, ServiceError> {
    let raw: Option<String> = state.redis.get(result_key(state, job_id)).await?;
    raw.map(|s| serde_json::from_str(&s).map_err(|e| ServiceError::internal(format!("parse result: {e}")))).transpose()
}

fn result_to_response(result: StoredResult) -> Result<Response, ServiceError> {
    let status = StatusCode::from_u16(result.http_status).unwrap_or(StatusCode::OK);
    let body = base64::engine::general_purpose::STANDARD.decode(result.body_base64).map_err(|e| ServiceError::internal(format!("decode result body: {e}")))?;
    let mut resp = (status, body).into_response();
    for (name, value) in result.headers {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(name.as_str()), HeaderValue::from_str(&value)) {
            resp.headers_mut().insert(name, value);
        }
    }
    resp.headers_mut().insert("x-job-id", header_value(&result.job_id)?);
    Ok(resp)
}

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
