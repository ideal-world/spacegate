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
use tokio::sync::Semaphore;
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
    #[arg(long, env = "AI_QUEUE_HIGH_STREAM", default_value = "ai:jobs:high")]
    high_priority_stream_key: String,
    #[arg(long, env = "AI_QUEUE_LOW_STREAM", default_value = "ai:jobs:low")]
    low_priority_stream_key: String,
    #[arg(long, env = "AI_ENABLE_PRIORITY_STREAMS", default_value_t = false)]
    enable_priority_streams: bool,
    #[arg(long, env = "AI_QUEUE_MAX_LEN", default_value_t = 100_000)]
    stream_max_len: u64,
    #[arg(long, env = "AI_QUEUE_GROUP", default_value = "ai-gateway-workers")]
    consumer_group: String,
    #[arg(long, env = "AI_QUEUE_CONSUMER", default_value = "ai-gateway-service")]
    consumer_name: String,
    #[arg(long, env = "AI_CALLBACK_RETRY_STREAM", default_value = "ai:callback-retry")]
    callback_retry_stream: String,
    #[arg(long, env = "AI_CALLBACK_RETRY_GROUP", default_value = "ai-gateway-callbacks")]
    callback_retry_group: String,
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
    #[arg(long, env = "AI_TENANT_RATE_LIMIT_PREFIX", default_value = "ai:tenant:ratelimit:")]
    tenant_rate_limit_prefix: String,
    #[arg(long, env = "AI_WAIT_TIMEOUT_SECS", default_value_t = 60)]
    wait_timeout_secs: u64,
    #[arg(long, env = "AI_WORKER_CONCURRENCY", default_value_t = 1)]
    worker_concurrency: usize,
    #[arg(long, env = "AI_UPSTREAM_BASE_URL")]
    upstream_base_url: Option<String>,
    #[arg(long, env = "AI_MAX_BODY_BYTES", default_value_t = 32 * 1024 * 1024)]
    max_body_bytes: usize,
    #[arg(long, env = "AI_INLINE_THRESHOLD", default_value_t = 128 * 1024)]
    inline_threshold: usize,
    #[arg(long, env = "AI_BODY_READ_CONCURRENCY", default_value_t = 200)]
    body_read_concurrency: usize,
    #[arg(long, env = "AI_RECLAIM_INTERVAL_SECS", default_value_t = 30)]
    reclaim_interval_secs: u64,
    #[arg(long, env = "AI_RECLAIM_MIN_IDLE_SECS", default_value_t = 30)]
    reclaim_min_idle_secs: u64,
    #[arg(long, env = "AI_REQUIRE_HTTPS_CALLBACK", default_value_t = true)]
    require_https_callback: bool,
    #[arg(long, env = "AI_OBJECT_STORE_ENDPOINT")]
    object_store_endpoint: Option<String>,
    #[arg(long, env = "AI_OBJECT_STORE_BUCKET", default_value = "ai-gateway-body")]
    object_store_bucket: String,
    #[arg(long, env = "AI_OBJECT_STORE_PREFIX", default_value = "bodies")]
    object_store_prefix: String,
    #[arg(long, env = "AI_OBJECT_MULTIPART_PART_SIZE", default_value_t = 5 * 1024 * 1024)]
    object_multipart_part_size: usize,
    #[arg(long, env = "AI_OBJECT_STORE_AUTH_HEADER")]
    object_store_auth_header: Option<String>,
}

#[derive(Clone)]
struct AppState {
    redis: FredClient,
    http: reqwest::Client,
    cfg: Arc<Args>,
    body_permits: Arc<Semaphore>,
    metrics: Arc<Metrics>,
}

#[derive(Default)]
struct Metrics {
    rate_limited_total: AtomicU64,
    enqueue_total: AtomicU64,
    enqueue_queue_total: AtomicU64,
    enqueue_wait_total: AtomicU64,
    wait_timeout_total: AtomicU64,
    callback_failure_total: AtomicU64,
    callback_retry_total: AtomicU64,
    callback_retry_success_total: AtomicU64,
    worker_completed_total: AtomicU64,
    worker_failed_total: AtomicU64,
    reclaimed_total: AtomicU64,
    object_offload_total: AtomicU64,
    object_multipart_abort_total: AtomicU64,
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
    stream_key: String,
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
struct AcceptedJob {
    response: EnqueueResponse,
    created_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueuePolicy {
    Queue,
    Wait,
}

impl QueuePolicy {
    fn as_str(self) -> &'static str {
        match self {
            QueuePolicy::Queue => "queue",
            QueuePolicy::Wait => "wait",
        }
    }
}

#[derive(Debug)]
struct BodyLocation {
    body_base64: String,
    object_ref: String,
    size: usize,
    storage: &'static str,
}

#[derive(Debug)]
struct CompletedPart {
    part_number: usize,
    etag: String,
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
        body_permits: Arc::new(Semaphore::new(args.body_read_concurrency.max(1))),
        metrics: Arc::new(Metrics::default()),
    };

    ensure_consumer_groups(&state).await?;
    if state.cfg.upstream_base_url.is_some() {
        spawn_workers(state.clone());
        spawn_reclaimer(state.clone());
        spawn_callback_retry_worker(state.clone());
    } else {
        tracing::warn!("AI_UPSTREAM_BASE_URL is not set; queue jobs will be stored but no local worker will process them");
    }

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics))
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
    let (rate_limit_rps, rate_limit_burst) = tenant_rate_limit(&state, &tenant).await?;
    let key = sanitize_key(&format!("{tenant}:{model}:{path}"));
    let tokens_key = format!("ai:ratelimit:{key}:tokens");
    let ts_key = format!("ai:ratelimit:{key}:ts");
    let now = now_ms();

    let out: Vec<i64> = state
        .redis
        .eval(
            TOKEN_BUCKET_LUA,
            vec![tokens_key, ts_key],
            vec![rate_limit_rps.to_string(), rate_limit_burst.to_string(), now.to_string(), "1".to_string()],
        )
        .await?;

    let allowed = out.first().copied().unwrap_or(0) == 1;
    if !allowed {
        state.metrics.rate_limited_total.fetch_add(1, Ordering::Relaxed);
    }
    Ok(Json(RateLimitResponse {
        allowed,
        remaining_tokens_milli: out.get(1).copied().unwrap_or(0),
        retry_after_ms: out.get(2).copied().unwrap_or(0),
    }))
}

async fn enqueue(State(state): State<AppState>, method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Result<impl IntoResponse, ServiceError> {
    let accepted = enqueue_job(&state, QueuePolicy::Queue, method, uri, headers, body).await?;
    let mut resp = (StatusCode::ACCEPTED, Json(&accepted.response)).into_response();
    resp.headers_mut().insert("x-job-id", header_value(&accepted.response.job_id)?);
    resp.headers_mut().insert("location", header_value(&accepted.response.poll_url)?);
    Ok(resp)
}

async fn enqueue_and_wait(State(state): State<AppState>, method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Result<Response, ServiceError> {
    let timeout_secs = optional_header(&headers, "x-request-timeout").and_then(|v| v.parse::<u64>().ok()).unwrap_or(state.cfg.wait_timeout_secs);
    let accepted = enqueue_job(&state, QueuePolicy::Wait, method, uri, headers, body).await?;
    let channel = result_channel(&state, &accepted.response.job_id);
    let subscriber = build_subscriber_client(&state.cfg.redis_url)?;
    let _subscriber_task = subscriber.init().await?;
    subscriber.subscribe(channel.as_str()).await?;

    if let Some(result) = load_result(&state, &accepted.response.job_id).await? {
        let _ = subscriber.quit().await;
        return Ok(result_to_response(result, accepted.created_at_ms)?);
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
            if let Some(result) = load_result(&state, &accepted.response.job_id).await? {
                Ok(result_to_response(result, accepted.created_at_ms)?)
            } else {
                Err(ServiceError::gateway_timeout(format!(
                    "job {} completed notification received but result is missing",
                    accepted.response.job_id
                )))
            }
        }
        _ => {
            let _ = subscriber.quit().await;
            state.metrics.wait_timeout_total.fetch_add(1, Ordering::Relaxed);
            let waited_ms = now_ms().saturating_sub(accepted.created_at_ms);
            let body = Json(serde_json::json!({
                "error": "timeout",
                "job_id": accepted.response.job_id,
                "poll_url": accepted.response.poll_url,
                "waited_ms": waited_ms,
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

async fn metrics(State(state): State<AppState>) -> Result<Response, ServiceError> {
    let queue_depth: i64 = state.redis.xlen(state.cfg.stream_key.as_str()).await.unwrap_or_default();
    let high_queue_depth: i64 = state.redis.xlen(state.cfg.high_priority_stream_key.as_str()).await.unwrap_or_default();
    let low_queue_depth: i64 = state.redis.xlen(state.cfg.low_priority_stream_key.as_str()).await.unwrap_or_default();
    let callback_retry_depth: i64 = state.redis.xlen(state.cfg.callback_retry_stream.as_str()).await.unwrap_or_default();

    let body = format!(
        "\
rate_limited_total {}\n\
enqueue_total {}\n\
enqueue_total{{policy=\"queue\"}} {}\n\
enqueue_total{{policy=\"wait\"}} {}\n\
wait_timeout_total {}\n\
callback_failure_total {}\n\
callback_retry_total {}\n\
callback_retry_success_total {}\n\
worker_completed_total {}\n\
worker_failed_total {}\n\
reclaimed_total {}\n\
object_offload_total {}\n\
object_multipart_abort_total {}\n\
queue_depth {}\n\
queue_depth{{priority=\"high\"}} {}\n\
queue_depth{{priority=\"low\"}} {}\n\
callback_retry_depth {}\n",
        state.metrics.rate_limited_total.load(Ordering::Relaxed),
        state.metrics.enqueue_total.load(Ordering::Relaxed),
        state.metrics.enqueue_queue_total.load(Ordering::Relaxed),
        state.metrics.enqueue_wait_total.load(Ordering::Relaxed),
        state.metrics.wait_timeout_total.load(Ordering::Relaxed),
        state.metrics.callback_failure_total.load(Ordering::Relaxed),
        state.metrics.callback_retry_total.load(Ordering::Relaxed),
        state.metrics.callback_retry_success_total.load(Ordering::Relaxed),
        state.metrics.worker_completed_total.load(Ordering::Relaxed),
        state.metrics.worker_failed_total.load(Ordering::Relaxed),
        state.metrics.reclaimed_total.load(Ordering::Relaxed),
        state.metrics.object_offload_total.load(Ordering::Relaxed),
        state.metrics.object_multipart_abort_total.load(Ordering::Relaxed),
        queue_depth,
        high_queue_depth,
        low_queue_depth,
        callback_retry_depth,
    );
    Ok((StatusCode::OK, [("content-type", "text/plain; version=0.0.4")], body).into_response())
}

async fn enqueue_job(state: &AppState, policy: QueuePolicy, _method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Result<AcceptedJob, ServiceError> {
    let _permit = state.body_permits.acquire().await.map_err(|_| ServiceError::internal("body semaphore closed"))?;
    let job_id = new_job_id();
    let tenant_id = required_header(&headers, "x-tenant-id")?;
    let model = optional_header(&headers, "x-model").unwrap_or_else(|| "default".to_string());
    let callback_url = optional_header(&headers, "x-callback-url").unwrap_or_default();
    validate_callback_url(state, policy, &callback_url)?;
    let original_method = optional_header(&headers, "x-original-method").unwrap_or_else(|| "POST".to_string());
    let original_path = optional_header(&headers, "x-original-path").unwrap_or_else(|| uri.path().to_string());
    let request_headers = headers_to_json(&headers)?;
    let created_at = now_ms();
    let body_ref = store_body(state, &job_id, body).await?;
    let stream_key = stream_for_request(state, &headers);

    let stream_id: String = state
        .redis
        .xadd(
            stream_key.as_str(),
            false,
            None::<()>,
            "*",
            vec![
                ("job_id", Value::String(job_id.clone().into())),
                ("tenant_id", Value::String(tenant_id.into())),
                ("policy", Value::String(policy.as_str().into())),
                ("model", Value::String(model.into())),
                ("method", Value::String(original_method.into())),
                ("path", Value::String(original_path.into())),
                ("headers", Value::String(request_headers.into())),
                ("body", Value::String(body_ref.body_base64.into())),
                ("ref", Value::String(body_ref.object_ref.into())),
                ("size", Value::Integer(body_ref.size as i64)),
                ("storage", Value::String(body_ref.storage.into())),
                ("callback_url", Value::String(callback_url.into())),
                ("created_at", Value::Integer(created_at as i64)),
            ],
        )
        .await?;
    trim_stream(state, &stream_key).await?;

    state.metrics.enqueue_total.fetch_add(1, Ordering::Relaxed);
    match policy {
        QueuePolicy::Queue => {
            state.metrics.enqueue_queue_total.fetch_add(1, Ordering::Relaxed);
        }
        QueuePolicy::Wait => {
            state.metrics.enqueue_wait_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    Ok(AcceptedJob {
        response: EnqueueResponse {
            job_id: job_id.clone(),
            stream_id,
            stream_key,
            status: "queued",
            poll_url: format!("/v1/jobs/{job_id}"),
        },
        created_at_ms: created_at,
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
    let streams = worker_streams(state);
    for (idx, stream) in streams.iter().enumerate() {
        let block = if idx + 1 == streams.len() { 1000 } else { 10 };
        let processed = read_worker_stream(state, consumer, stream, block).await?;
        if processed > 0 {
            return Ok(());
        }
    }
    Ok(())
}

async fn read_worker_stream(state: &AppState, consumer: &str, stream: &str, block_ms: u64) -> Result<usize, ServiceError> {
    let reply: XReadResponse<String, String, String, Value> =
        state.redis.xreadgroup_map(state.cfg.consumer_group.as_str(), consumer, Some(5), Some(block_ms), false, vec![stream], vec![">"]).await?;

    let mut processed = 0;
    for (_stream, entries) in reply {
        for (entry_id, fields) in entries {
            match process_job(state, stream, entry_id.as_str(), &fields).await {
                Ok(()) => {
                    let _: i64 = state.redis.xack(stream, state.cfg.consumer_group.as_str(), vec![entry_id.clone()]).await?;
                    processed += 1;
                }
                Err(e) => {
                    tracing::warn!(stream_id = %entry_id, error = %e.message, "job processing failed");
                    state.metrics.worker_failed_total.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    Ok(processed)
}

async fn process_job(state: &AppState, _stream: &str, _stream_id: &str, fields: &HashMap<String, Value>) -> Result<(), ServiceError> {
    let Some(base) = state.cfg.upstream_base_url.as_deref() else {
        return Err(ServiceError::internal("upstream base URL is not configured"));
    };
    let job_id = field_string(fields, "job_id").ok_or_else(|| ServiceError::bad_request("job missing job_id"))?;
    let method = field_string(fields, "method").unwrap_or_else(|| "POST".to_string());
    let path = field_string(fields, "path").unwrap_or_else(|| "/".to_string());
    let headers_json = field_string(fields, "headers").unwrap_or_else(|| "{}".to_string());
    let callback_url = field_string(fields, "callback_url").unwrap_or_default();
    let body = load_body(state, fields).await?;
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
        let callback_body = callback_body(&result);
        if let Err(e) = post_callback(state, &callback_url, &job_id, &callback_body).await {
            tracing::warn!(job_id = %job_id, error = %e.message, "callback failed");
            state.metrics.callback_failure_total.fetch_add(1, Ordering::Relaxed);
            enqueue_callback_retry(state, &callback_url, &job_id, &callback_body).await?;
        }
    }
    state.metrics.worker_completed_total.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

fn callback_body(result: &StoredResult) -> serde_json::Value {
    serde_json::json!({
        "job_id": result.job_id,
        "status": result.status,
        "http_status": result.http_status,
        "headers": result.headers,
        "body_base64": result.body_base64,
        "result": result.body_base64,
        "completed_at_ms": result.completed_at_ms,
        "error": result.error,
    })
}

async fn post_callback(state: &AppState, callback_url: &str, job_id: &str, body: &serde_json::Value) -> Result<(), ServiceError> {
    state.http.post(callback_url).header("x-gateway-job-id", job_id).json(body).send().await?.error_for_status()?;
    Ok(())
}

async fn enqueue_callback_retry(state: &AppState, callback_url: &str, job_id: &str, body: &serde_json::Value) -> Result<(), ServiceError> {
    let body = serde_json::to_string(body).map_err(|e| ServiceError::internal(format!("serialize callback retry: {e}")))?;
    let _: String = state
        .redis
        .xadd(
            state.cfg.callback_retry_stream.as_str(),
            false,
            None::<()>,
            "*",
            vec![
                ("job_id", Value::String(job_id.to_string().into())),
                ("callback_url", Value::String(callback_url.to_string().into())),
                ("body", Value::String(body.into())),
                ("created_at", Value::Integer(now_ms() as i64)),
            ],
        )
        .await?;
    trim_stream(state, &state.cfg.callback_retry_stream).await?;
    state.metrics.callback_retry_total.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

fn spawn_callback_retry_worker(state: AppState) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = callback_retry_once(&state).await {
                tracing::warn!(error = %e.message, "callback retry loop failed");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    });
}

async fn callback_retry_once(state: &AppState) -> Result<(), ServiceError> {
    let reply: XReadResponse<String, String, String, Value> = state
        .redis
        .xreadgroup_map(
            state.cfg.callback_retry_group.as_str(),
            state.cfg.consumer_name.as_str(),
            Some(5),
            Some(1000),
            false,
            vec![state.cfg.callback_retry_stream.as_str()],
            vec![">"],
        )
        .await?;

    for (_stream, entries) in reply {
        for (entry_id, fields) in entries {
            let job_id = field_string(&fields, "job_id").unwrap_or_default();
            let callback_url = field_string(&fields, "callback_url").unwrap_or_default();
            let body = field_string(&fields, "body").unwrap_or_else(|| "{}".to_string());
            let parsed = serde_json::from_str::<serde_json::Value>(&body).unwrap_or_else(|_| serde_json::json!({ "body": body }));
            match post_callback(state, &callback_url, &job_id, &parsed).await {
                Ok(()) => {
                    let _: i64 = state.redis.xack(state.cfg.callback_retry_stream.as_str(), state.cfg.callback_retry_group.as_str(), vec![entry_id]).await?;
                    state.metrics.callback_retry_success_total.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    tracing::warn!(job_id = %job_id, error = %e.message, "callback retry failed");
                }
            }
        }
    }
    Ok(())
}

fn spawn_reclaimer(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(state.cfg.reclaim_interval_secs.max(1)));
        loop {
            interval.tick().await;
            if let Err(e) = reclaim_once(&state).await {
                tracing::warn!(error = %e.message, "stream reclaim failed");
            }
        }
    });
}

async fn reclaim_once(state: &AppState) -> Result<(), ServiceError> {
    let consumer = format!("{}-reclaimer", state.cfg.consumer_name);
    let min_idle_ms = state.cfg.reclaim_min_idle_secs.saturating_mul(1000);
    for stream in worker_streams(state) {
        let (_cursor, entries): (String, Vec<(String, HashMap<String, Value>)>) =
            state.redis.xautoclaim_values(stream.as_str(), state.cfg.consumer_group.as_str(), consumer.as_str(), min_idle_ms, "0-0", Some(10), false).await?;
        for (entry_id, fields) in entries {
            match process_job(state, stream.as_str(), entry_id.as_str(), &fields).await {
                Ok(()) => {
                    let _: i64 = state.redis.xack(stream.as_str(), state.cfg.consumer_group.as_str(), vec![entry_id]).await?;
                    state.metrics.reclaimed_total.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    tracing::warn!(stream = %stream, entry_id = %entry_id, error = %e.message, "reclaimed job failed");
                }
            }
        }
    }
    Ok(())
}

async fn ensure_consumer_groups(state: &AppState) -> Result<(), ServiceError> {
    for stream in worker_streams(state) {
        ensure_consumer_group(state, &stream, &state.cfg.consumer_group).await?;
    }
    ensure_consumer_group(state, &state.cfg.callback_retry_stream, &state.cfg.callback_retry_group).await?;
    Ok(())
}

async fn ensure_consumer_group(state: &AppState, stream: &str, group: &str) -> Result<(), ServiceError> {
    let res: FredResult<String> = state.redis.xgroup_create(stream, group, "$", true).await;
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

fn result_to_response(result: StoredResult, created_at_ms: u64) -> Result<Response, ServiceError> {
    let status = StatusCode::from_u16(result.http_status).unwrap_or(StatusCode::OK);
    let body = base64::engine::general_purpose::STANDARD.decode(result.body_base64).map_err(|e| ServiceError::internal(format!("decode result body: {e}")))?;
    let mut resp = (status, body).into_response();
    for (name, value) in result.headers {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(name.as_str()), HeaderValue::from_str(&value)) {
            resp.headers_mut().insert(name, value);
        }
    }
    resp.headers_mut().insert("x-job-id", header_value(&result.job_id)?);
    resp.headers_mut().insert("x-queue-wait-ms", header_value(&now_ms().saturating_sub(created_at_ms).to_string())?);
    Ok(resp)
}

async fn store_body(state: &AppState, job_id: &str, body: Bytes) -> Result<BodyLocation, ServiceError> {
    if body.len() <= state.cfg.inline_threshold || state.cfg.object_store_endpoint.is_none() {
        return Ok(BodyLocation {
            body_base64: base64::engine::general_purpose::STANDARD.encode(&body),
            object_ref: String::new(),
            size: body.len(),
            storage: "inline",
        });
    }

    let object_ref = format!("{}/{}/body.bin", state.cfg.object_store_prefix.trim_matches('/'), sanitize_key(job_id));
    multipart_upload_body(state, &object_ref, body.clone()).await?;
    state.metrics.object_offload_total.fetch_add(1, Ordering::Relaxed);
    Ok(BodyLocation {
        body_base64: String::new(),
        object_ref,
        size: body.len(),
        storage: "object",
    })
}

async fn load_body(state: &AppState, fields: &HashMap<String, Value>) -> Result<Vec<u8>, ServiceError> {
    let storage = field_string(fields, "storage").unwrap_or_else(|| "inline".to_string());
    if storage == "object" {
        let object_ref = field_string(fields, "ref").ok_or_else(|| ServiceError::bad_request("job body is missing object ref"))?;
        let url = object_url(state, &object_ref);
        let mut req = state.http.get(url);
        if let Some((name, value)) = object_auth_header(&state.cfg.object_store_auth_header)? {
            req = req.header(name, value);
        }
        return Ok(req.send().await?.error_for_status()?.bytes().await?.to_vec());
    }

    if let Some(body_base64) = field_string(fields, "body") {
        return base64::engine::general_purpose::STANDARD.decode(body_base64).map_err(|e| ServiceError::bad_request(format!("decode job body: {e}")));
    }
    Ok(field_bytes(fields, "body").unwrap_or_default())
}

async fn multipart_upload_body(state: &AppState, object_ref: &str, body: Bytes) -> Result<(), ServiceError> {
    let upload_id = initiate_multipart_upload(state, object_ref).await?;
    let upload_result = async {
        let part_size = state.cfg.object_multipart_part_size.max(5 * 1024 * 1024);
        let mut parts = Vec::new();
        for (idx, chunk) in body.chunks(part_size).enumerate() {
            parts.push(upload_multipart_part(state, object_ref, &upload_id, idx + 1, chunk.to_vec()).await?);
        }
        complete_multipart_upload(state, object_ref, &upload_id, &parts).await
    }
    .await;

    if let Err(err) = upload_result {
        if let Err(abort_err) = abort_multipart_upload(state, object_ref, &upload_id).await {
            state.metrics.object_multipart_abort_total.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(object_ref = %object_ref, upload_id = %upload_id, error = %abort_err.message, "multipart upload abort failed");
        } else {
            state.metrics.object_multipart_abort_total.fetch_add(1, Ordering::Relaxed);
        }
        return Err(err);
    }
    Ok(())
}

async fn initiate_multipart_upload(state: &AppState, object_ref: &str) -> Result<String, ServiceError> {
    let url = object_url_with_query(state, object_ref, "uploads");
    let mut req = state.http.post(url);
    if let Some((name, value)) = object_auth_header(&state.cfg.object_store_auth_header)? {
        req = req.header(name, value);
    }
    let body = req.send().await?.error_for_status()?.text().await?;
    extract_xml_tag(&body, "UploadId").ok_or_else(|| ServiceError::internal("multipart initiate response missing UploadId"))
}

async fn upload_multipart_part(state: &AppState, object_ref: &str, upload_id: &str, part_number: usize, body: Vec<u8>) -> Result<CompletedPart, ServiceError> {
    let query = format!("partNumber={part_number}&uploadId={}", encode_query_component(upload_id));
    let url = object_url_with_query(state, object_ref, &query);
    let mut req = state.http.put(url).body(body);
    if let Some((name, value)) = object_auth_header(&state.cfg.object_store_auth_header)? {
        req = req.header(name, value);
    }
    let resp = req.send().await?.error_for_status()?;
    let etag = resp
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ServiceError::internal("multipart upload part response missing ETag"))?;
    Ok(CompletedPart { part_number, etag })
}

async fn complete_multipart_upload(state: &AppState, object_ref: &str, upload_id: &str, parts: &[CompletedPart]) -> Result<(), ServiceError> {
    let query = format!("uploadId={}", encode_query_component(upload_id));
    let url = object_url_with_query(state, object_ref, &query);
    let body = complete_multipart_xml(parts);
    let mut req = state.http.post(url).header("content-type", "application/xml").body(body);
    if let Some((name, value)) = object_auth_header(&state.cfg.object_store_auth_header)? {
        req = req.header(name, value);
    }
    req.send().await?.error_for_status()?;
    Ok(())
}

async fn abort_multipart_upload(state: &AppState, object_ref: &str, upload_id: &str) -> Result<(), ServiceError> {
    let query = format!("uploadId={}", encode_query_component(upload_id));
    let url = object_url_with_query(state, object_ref, &query);
    let mut req = state.http.delete(url);
    if let Some((name, value)) = object_auth_header(&state.cfg.object_store_auth_header)? {
        req = req.header(name, value);
    }
    req.send().await?.error_for_status()?;
    Ok(())
}

fn complete_multipart_xml(parts: &[CompletedPart]) -> String {
    let mut out = String::from("<CompleteMultipartUpload>");
    for part in parts {
        out.push_str("<Part>");
        out.push_str("<PartNumber>");
        out.push_str(&part.part_number.to_string());
        out.push_str("</PartNumber>");
        out.push_str("<ETag>");
        out.push_str(&xml_escape(&part.etag));
        out.push_str("</ETag>");
        out.push_str("</Part>");
    }
    out.push_str("</CompleteMultipartUpload>");
    out
}

fn object_url(state: &AppState, object_ref: &str) -> String {
    format!(
        "{}/{}/{}",
        state.cfg.object_store_endpoint.as_deref().unwrap_or_default().trim_end_matches('/'),
        state.cfg.object_store_bucket.trim_matches('/'),
        object_ref.trim_start_matches('/')
    )
}

fn object_url_with_query(state: &AppState, object_ref: &str, query: &str) -> String {
    format!("{}?{}", object_url(state, object_ref), query)
}

fn object_auth_header(raw: &Option<String>) -> Result<Option<(String, String)>, ServiceError> {
    let Some(raw) = raw.as_deref() else {
        return Ok(None);
    };
    let Some((name, value)) = raw.split_once(':') else {
        return Err(ServiceError::bad_request("AI_OBJECT_STORE_AUTH_HEADER must be `Header-Name: value`"));
    };
    if HeaderName::try_from(name.trim()).is_err() || HeaderValue::from_str(value.trim()).is_err() {
        return Err(ServiceError::bad_request("invalid object auth header"));
    }
    Ok(Some((name.trim().to_string(), value.trim().to_string())))
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{tag}>");
    let end_tag = format!("</{tag}>");
    let start = xml.find(&start_tag)? + start_tag.len();
    let end = xml[start..].find(&end_tag)? + start;
    Some(xml[start..end].trim().to_string())
}

fn encode_query_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn xml_escape(input: &str) -> String {
    input.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

async fn trim_stream(state: &AppState, stream: &str) -> Result<(), ServiceError> {
    if state.cfg.stream_max_len > 0 {
        let _: i64 = state.redis.xtrim(stream, ("MAXLEN", "~", state.cfg.stream_max_len as i64)).await?;
    }
    Ok(())
}

fn stream_for_request(state: &AppState, headers: &HeaderMap) -> String {
    if !state.cfg.enable_priority_streams {
        return state.cfg.stream_key.clone();
    }
    match optional_header(headers, "x-queue-priority").as_deref() {
        Some("high") => state.cfg.high_priority_stream_key.clone(),
        Some("low") => state.cfg.low_priority_stream_key.clone(),
        _ => state.cfg.stream_key.clone(),
    }
}

fn worker_streams(state: &AppState) -> Vec<String> {
    if state.cfg.enable_priority_streams {
        vec![
            state.cfg.high_priority_stream_key.clone(),
            state.cfg.stream_key.clone(),
            state.cfg.low_priority_stream_key.clone(),
        ]
    } else {
        vec![state.cfg.stream_key.clone()]
    }
}

fn validate_callback_url(state: &AppState, policy: QueuePolicy, callback_url: &str) -> Result<(), ServiceError> {
    if policy == QueuePolicy::Queue && callback_url.is_empty() {
        return Err(ServiceError::bad_request("missing required header `x-callback-url` for queue policy"));
    }
    if !callback_url.is_empty() && state.cfg.require_https_callback && !callback_url.starts_with("https://") {
        return Err(ServiceError::bad_request("x-callback-url must use https"));
    }
    Ok(())
}

async fn tenant_rate_limit(state: &AppState, tenant: &str) -> Result<(u64, u64), ServiceError> {
    let key = format!("{}{}", state.cfg.tenant_rate_limit_prefix, sanitize_key(tenant));
    let rps: Option<String> = state.redis.get(format!("{key}:rps")).await.unwrap_or(None);
    let burst: Option<String> = state.redis.get(format!("{key}:burst")).await.unwrap_or(None);
    Ok((
        rps.and_then(|v| v.parse().ok()).unwrap_or(state.cfg.rate_limit_rps),
        burst.and_then(|v| v.parse().ok()).unwrap_or(state.cfg.rate_limit_burst),
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_upload_id_from_multipart_xml() {
        let xml = "<InitiateMultipartUploadResult><UploadId>a+b/c=</UploadId></InitiateMultipartUploadResult>";
        assert_eq!(extract_xml_tag(xml, "UploadId").as_deref(), Some("a+b/c="));
    }

    #[test]
    fn encodes_upload_id_for_query_string() {
        assert_eq!(encode_query_component("a+b/c="), "a%2Bb%2Fc%3D");
    }

    #[test]
    fn builds_complete_multipart_xml_with_escaped_etags() {
        let parts = vec![
            CompletedPart {
                part_number: 1,
                etag: "\"abc&1\"".to_string(),
            },
            CompletedPart {
                part_number: 2,
                etag: "\"def\"".to_string(),
            },
        ];
        let xml = complete_multipart_xml(&parts);
        assert!(xml.contains("<PartNumber>1</PartNumber><ETag>&quot;abc&amp;1&quot;</ETag>"));
        assert!(xml.contains("<PartNumber>2</PartNumber><ETag>&quot;def&quot;</ETag>"));
    }
}
