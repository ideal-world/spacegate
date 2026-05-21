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
    #[arg(long, env = "AI_QUEUE_DEFAULT_PRIORITY", default_value = "normal")]
    queue_default_priority: String,
    #[arg(long, env = "AI_QUEUE_HIGH_MODELS", default_value = "")]
    queue_high_models: String,
    #[arg(long, env = "AI_QUEUE_LOW_MODELS", default_value = "")]
    queue_low_models: String,
    #[arg(long, env = "AI_QUEUE_HIGH_TENANTS", default_value = "")]
    queue_high_tenants: String,
    #[arg(long, env = "AI_QUEUE_LOW_TENANTS", default_value = "")]
    queue_low_tenants: String,
    #[arg(long, env = "AI_QUEUE_HIGH_WEIGHT", default_value_t = 3)]
    queue_high_weight: usize,
    #[arg(long, env = "AI_QUEUE_NORMAL_WEIGHT", default_value_t = 1)]
    queue_normal_weight: usize,
    #[arg(long, env = "AI_QUEUE_LOW_WEIGHT", default_value_t = 1)]
    queue_low_weight: usize,
    #[arg(long, env = "AI_QUEUE_MAX_LEN", default_value_t = 100_000)]
    stream_max_len: u64,
    #[arg(long, env = "AI_QUEUE_GROUP", default_value = "ai-gateway-workers")]
    consumer_group: String,
    #[arg(long, env = "AI_QUEUE_CONSUMER", default_value = "ai-gateway-service")]
    consumer_name: String,
    #[arg(long, env = "AI_JOB_DLQ_STREAM", default_value = "ai:job-dlq")]
    job_dlq_stream: String,
    #[arg(long, env = "AI_CALLBACK_RETRY_STREAM", default_value = "ai:callback-retry")]
    callback_retry_stream: String,
    #[arg(long, env = "AI_CALLBACK_RETRY_GROUP", default_value = "ai-gateway-callbacks")]
    callback_retry_group: String,
    #[arg(long, env = "AI_CALLBACK_DLQ_STREAM", default_value = "ai:callback-dlq")]
    callback_dlq_stream: String,
    #[arg(long, env = "AI_CALLBACK_MAX_RETRY_ATTEMPTS", default_value_t = 5)]
    callback_max_retry_attempts: u32,
    #[arg(long, env = "AI_CALLBACK_RETRY_INITIAL_DELAY_MS", default_value_t = 1000)]
    callback_retry_initial_delay_ms: u64,
    #[arg(long, env = "AI_CALLBACK_RETRY_MAX_DELAY_MS", default_value_t = 60_000)]
    callback_retry_max_delay_ms: u64,
    #[arg(long, env = "AI_CALLBACK_RETRY_RECLAIM_IDLE_SECS", default_value_t = 60)]
    callback_retry_reclaim_idle_secs: u64,
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
    #[arg(long, env = "AI_RATE_LIMIT_COST", default_value_t = 1)]
    rate_limit_cost: u64,
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
    #[arg(long, env = "AI_JOB_PROCESS_LEASE_SECS", default_value_t = 120)]
    job_process_lease_secs: u64,
    #[arg(long, env = "AI_JOB_MAX_DELIVERY_ATTEMPTS", default_value_t = 5)]
    job_max_delivery_attempts: u32,
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
    enqueue_priority_high_total: AtomicU64,
    enqueue_priority_normal_total: AtomicU64,
    enqueue_priority_low_total: AtomicU64,
    enqueue_latency_count: AtomicU64,
    enqueue_latency_sum_ms: AtomicU64,
    enqueue_latency_le_100_ms: AtomicU64,
    enqueue_latency_le_500_ms: AtomicU64,
    enqueue_latency_le_1000_ms: AtomicU64,
    enqueue_latency_gt_1000_ms: AtomicU64,
    body_size_le_10kb: AtomicU64,
    body_size_le_128kb: AtomicU64,
    body_size_le_5mb: AtomicU64,
    body_size_gt_5mb: AtomicU64,
    body_size_count: AtomicU64,
    body_size_sum_bytes: AtomicU64,
    wait_total: AtomicU64,
    wait_timeout_total: AtomicU64,
    callback_failure_total: AtomicU64,
    callback_retry_total: AtomicU64,
    callback_retry_success_total: AtomicU64,
    callback_retry_dlq_total: AtomicU64,
    worker_completed_total: AtomicU64,
    worker_failed_total: AtomicU64,
    worker_processing_count: AtomicU64,
    worker_processing_sum_ms: AtomicU64,
    worker_processing_le_1000_ms: AtomicU64,
    worker_processing_le_5000_ms: AtomicU64,
    worker_processing_le_30000_ms: AtomicU64,
    worker_processing_gt_30000_ms: AtomicU64,
    reclaimed_total: AtomicU64,
    job_dlq_total: AtomicU64,
    lease_skip_total: AtomicU64,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueuePriority {
    High,
    Normal,
    Low,
}

impl QueuePriority {
    fn as_str(self) -> &'static str {
        match self {
            QueuePriority::High => "high",
            QueuePriority::Normal => "normal",
            QueuePriority::Low => "low",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
struct TenantRateLimit {
    rps: u64,
    burst: u64,
    #[serde(default = "default_rate_limit_cost")]
    cost: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct AiGatewayQueuePluginConfig {
    #[serde(default)]
    service: AiGatewayServiceConfig,
    #[serde(default)]
    paths: AiGatewayPathsConfig,
    #[serde(default)]
    headers: AiGatewayHeadersConfig,
    #[serde(default)]
    policies: AiGatewayPoliciesConfig,
    #[serde(default)]
    priority: AiGatewayPriorityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct AiGatewayServiceConfig {
    #[serde(default = "default_service_cluster")]
    cluster: String,
    #[serde(default = "default_service_authority")]
    authority: String,
    #[serde(default = "default_service_timeout_ms")]
    timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct AiGatewayPathsConfig {
    #[serde(default = "default_rate_limit_path")]
    rate_limit: String,
    #[serde(default = "default_enqueue_path")]
    enqueue: String,
    #[serde(default = "default_wait_path")]
    wait: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct AiGatewayHeadersConfig {
    #[serde(default = "default_policy_header")]
    policy: String,
    #[serde(default = "default_tenant_header")]
    tenant: String,
    #[serde(default = "default_model_header")]
    model: String,
    #[serde(default = "default_priority_header")]
    priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct AiGatewayPoliciesConfig {
    #[serde(default = "default_require_policy")]
    require: bool,
    #[serde(default)]
    default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct AiGatewayPriorityConfig {
    #[serde(default = "default_priority_enabled")]
    enabled: bool,
    #[serde(default = "default_priority")]
    default: String,
    #[serde(default)]
    high_models: Vec<String>,
    #[serde(default)]
    low_models: Vec<String>,
    #[serde(default)]
    high_tenants: Vec<String>,
    #[serde(default)]
    low_tenants: Vec<String>,
}

impl Default for AiGatewayQueuePluginConfig {
    fn default() -> Self {
        Self {
            service: AiGatewayServiceConfig::default(),
            paths: AiGatewayPathsConfig::default(),
            headers: AiGatewayHeadersConfig::default(),
            policies: AiGatewayPoliciesConfig::default(),
            priority: AiGatewayPriorityConfig::default(),
        }
    }
}

impl Default for AiGatewayServiceConfig {
    fn default() -> Self {
        Self {
            cluster: default_service_cluster(),
            authority: default_service_authority(),
            timeout_ms: default_service_timeout_ms(),
        }
    }
}

impl Default for AiGatewayPathsConfig {
    fn default() -> Self {
        Self {
            rate_limit: default_rate_limit_path(),
            enqueue: default_enqueue_path(),
            wait: default_wait_path(),
        }
    }
}

impl Default for AiGatewayHeadersConfig {
    fn default() -> Self {
        Self {
            policy: default_policy_header(),
            tenant: default_tenant_header(),
            model: default_model_header(),
            priority: default_priority_header(),
        }
    }
}

impl Default for AiGatewayPoliciesConfig {
    fn default() -> Self {
        Self {
            require: default_require_policy(),
            default: None,
        }
    }
}

impl Default for AiGatewayPriorityConfig {
    fn default() -> Self {
        Self {
            enabled: default_priority_enabled(),
            default: default_priority(),
            high_models: Vec::new(),
            low_models: Vec::new(),
            high_tenants: Vec::new(),
            low_tenants: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct TenantRateLimitRule {
    tenant: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    policy: Option<String>,
    rps: u64,
    burst: u64,
    #[serde(default = "default_rate_limit_cost")]
    cost: u64,
    // TODO(v2): apply ttl_secs to the Redis key via EXPIRE/PEXPIRE when time-bound rules are enabled.
    #[serde(default)]
    ttl_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct TenantRateLimitRuleView {
    key: String,
    #[serde(flatten)]
    rule: TenantRateLimitRule,
}

fn default_rate_limit_cost() -> u64 {
    1
}

fn default_service_cluster() -> String {
    "ai-gateway-service".to_string()
}

fn default_service_authority() -> String {
    "ai-gateway-service".to_string()
}

fn default_service_timeout_ms() -> u64 {
    65_000
}

fn default_rate_limit_path() -> String {
    "/v1/ratelimit/check".to_string()
}

fn default_enqueue_path() -> String {
    "/v1/queue/enqueue".to_string()
}

fn default_wait_path() -> String {
    "/v1/queue/enqueue-and-wait".to_string()
}

fn default_policy_header() -> String {
    "x-ratelimit-policy".to_string()
}

fn default_tenant_header() -> String {
    "x-tenant-id".to_string()
}

fn default_model_header() -> String {
    "x-model".to_string()
}

fn default_priority_header() -> String {
    "x-queue-priority".to_string()
}

fn default_require_policy() -> bool {
    true
}

fn default_priority_enabled() -> bool {
    true
}

fn default_priority() -> String {
    "normal".to_string()
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

    fn payload_too_large(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
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
