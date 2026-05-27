// 集成测试 harness：启动 mock 上游/回调、隔离 Redis key、进程内 HTTP 服务。
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use fred::prelude::*;
use futures_util::StreamExt;
use reqwest::Client;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use super::*;

/// 集成测试可选配置（字段公开，供外部 integration crate 使用）。
#[derive(Default, Clone)]
pub struct HarnessConfig {
    pub rate_limit_rps: Option<u64>,
    pub rate_limit_burst: Option<u64>,
    pub wait_timeout_secs: Option<u64>,
    pub require_https_callback: Option<bool>,
    pub inline_threshold: Option<usize>,
    pub clear_object_store: bool,
}

impl HarnessConfig {
    fn apply(self, args: &mut Args) {
        if let Some(v) = self.rate_limit_rps {
            args.rate_limit_rps = v;
        }
        if let Some(v) = self.rate_limit_burst {
            args.rate_limit_burst = v;
        }
        if let Some(v) = self.wait_timeout_secs {
            args.wait_timeout_secs = v;
        }
        if let Some(v) = self.require_https_callback {
            args.require_https_callback = v;
        }
        if let Some(v) = self.inline_threshold {
            args.inline_threshold = v;
        }
        if self.clear_object_store {
            args.object_store_endpoint = None;
        }
    }
}

/// 回调服务器记录到的 POST 请求。
#[derive(Debug, Clone, Default)]
pub struct CallbackRecord {
    pub job_id: String,
    pub body: serde_json::Value,
    pub headers: Vec<(String, String)>,
}

/// 集成测试环境：随机端口 HTTP 服务 + mock upstream/callback。
pub struct TestHarness {
    pub base_url: String,
    pub client: Client,
    pub state: AppState,
    pub upstream_url: String,
    pub callback_url: String,
    pub redis: FredClient,
    pub suffix: String,
    _server: JoinHandle<()>,
    _upstream: JoinHandle<()>,
    _callback: JoinHandle<()>,
    callback_records: Arc<Mutex<Vec<CallbackRecord>>>,
}

impl TestHarness {
    /// 使用默认 Redis（`REDIS_URL` 或 `redis://127.0.0.1/`）启动隔离测试环境。
    pub async fn start() -> Self {
        Self::start_with(|_| {}).await
    }

    /// 使用 [`HarnessConfig`] 启动（供 tests/integration 使用）。
    pub async fn start_config(config: HarnessConfig) -> Self {
        Self::start_with(move |a| {
            config.apply(a);
        })
        .await
    }

    /// 允许调用方微调 Args（限流、timeout、stream key 等）。
    pub async fn start_with(configure: impl FnOnce(&mut Args)) -> Self {
        if !redis_available().await {
            panic!("Redis 7+ is required for integration tests (set REDIS_URL or start redis locally)");
        }

        let suffix = ulid::Ulid::new().to_string().to_ascii_lowercase();
        let mut args = Args::parse_from(["ai-gateway-service"]);
        args.stream_key = format!("ai:jobs:test:{suffix}");
        args.high_priority_stream_key = format!("ai:jobs:high:test:{suffix}");
        args.low_priority_stream_key = format!("ai:jobs:low:test:{suffix}");
        args.consumer_group = format!("ai-gateway-workers-test-{suffix}");
        args.consumer_name = format!("ai-gateway-test-{suffix}");
        args.job_dlq_stream = format!("ai:job-dlq:test:{suffix}");
        args.callback_retry_stream = format!("ai:callback-retry:test:{suffix}");
        args.callback_retry_group = format!("ai-gateway-callbacks-test-{suffix}");
        args.callback_dlq_stream = format!("ai:callback-dlq:test:{suffix}");
        args.tenant_rate_limit_prefix = format!("ai:tenant:ratelimit:test:{suffix}:");
        args.result_key_prefix = format!("result:test:{suffix}:");
        args.result_channel_prefix = format!("result:test:{suffix}:");
        args.rate_limit_rps = 100;
        args.rate_limit_burst = 2;
        args.wait_timeout_secs = 3;
        args.reclaim_interval_secs = 2;
        args.reclaim_min_idle_secs = 1;
        args.worker_concurrency = 2;
        args.enable_priority_streams = true;
        // mock 回调为 http://127.0.0.1，测试环境关闭 HTTPS 强制
        args.require_https_callback = false;
        configure(&mut args);

        let (upstream_url, upstream_task) = spawn_mock_upstream(Duration::from_millis(50)).await;
        args.upstream_base_url = Some(upstream_url.clone());

        let callback_records = Arc::new(Mutex::new(Vec::new()));
        let (callback_url, callback_task) = spawn_mock_callback(callback_records.clone()).await;

        let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
        let redis = build_redis_client(&redis_url).expect("redis client");
        redis.init().await.expect("redis init");
        let worker_redis = build_redis_client(&redis_url).expect("worker redis");
        worker_redis.init().await.expect("worker redis init");
        let wait_subscriber = WaitSubscriberHub::new(&redis_url).await.expect("wait subscriber");

        let state = AppState {
            redis: redis.clone(),
            worker_redis,
            http: Client::new(),
            cfg: Arc::new(args.clone()),
            body_permits: Arc::new(Semaphore::new(args.body_read_concurrency.max(1))),
            metrics: Arc::new(Metrics::default()),
            wait_subscriber,
        };

        ensure_consumer_groups(&state).await.expect("consumer groups");
        spawn_workers(state.clone());
        spawn_reclaimer(state.clone());
        spawn_callback_retry_worker(state.clone());

        let app = build_router(state.clone(), args.max_body_bytes);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind harness");
        let addr = listener.local_addr().expect("local addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve harness");
        });

        let base_url = format!("http://{addr}");
        // 等待 worker 与 HTTP 就绪
        tokio::time::sleep(Duration::from_millis(100)).await;

        Self {
            base_url,
            client: Client::new(),
            state,
            upstream_url,
            callback_url,
            redis,
            suffix,
            _server: server,
            _upstream: upstream_task,
            _callback: callback_task,
            callback_records,
        }
    }

    pub fn callback_records(&self) -> Vec<CallbackRecord> {
        self.callback_records.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 查询 tenant 限流 Redis key（TC-RL-07）。
    pub async fn ratelimit_keys_for_tenant(&self, tenant: &str) -> Vec<String> {
        let pattern = format!("ai:ratelimit:{tenant}:*");
        let mut stream = self.redis.scan_buffered(pattern, Some(100), None);
        let mut out = Vec::new();
        while let Some(key) = stream.next().await {
            if let Ok(key) = key {
                out.push(key.into_string().unwrap_or_default());
            }
        }
        out
    }

    /// callback retry stream 深度。
    pub async fn callback_retry_depth(&self) -> i64 {
        self.redis
            .xlen(self.state.cfg.callback_retry_stream.as_str())
            .await
            .unwrap_or(0)
    }

    /// POST /v1/ratelimit/check
    pub async fn check_rate_limit(&self, tenant: &str, policy: &str) -> reqwest::Response {
        self.client
            .post(format!("{}/v1/ratelimit/check", self.base_url))
            .header("x-tenant-id", tenant)
            .header("x-ratelimit-policy", policy)
            .header("x-original-path", "/v1/chat")
            .send()
            .await
            .expect("rate limit request")
    }

    /// POST /v1/queue/enqueue
    pub async fn enqueue(&self, tenant: &str, body: Vec<u8>, extra: HeaderMap) -> reqwest::Response {
        let mut req = self
            .client
            .post(format!("{}/v1/queue/enqueue", self.base_url))
            .header("x-tenant-id", tenant)
            .header("x-ratelimit-policy", "queue")
            .header("x-callback-url", &self.callback_url)
            .header("x-original-method", "POST")
            .header("x-original-path", "/v1/chat");
        for (k, v) in extra.iter() {
            if let Ok(v) = v.to_str() {
                req = req.header(k.as_str(), v);
            }
        }
        req.body(body).send().await.expect("enqueue")
    }

    /// POST /v1/queue/enqueue-and-wait
    pub async fn enqueue_and_wait(&self, tenant: &str, body: Vec<u8>, timeout_secs: Option<u64>) -> reqwest::Response {
        let mut req = self
            .client
            .post(format!("{}/v1/queue/enqueue-and-wait", self.base_url))
            .header("x-tenant-id", tenant)
            .header("x-ratelimit-policy", "wait")
            .header("x-original-method", "POST")
            .header("x-original-path", "/v1/chat");
        if let Some(secs) = timeout_secs {
            req = req.header("x-request-timeout", secs.to_string());
        }
        req.body(body).send().await.expect("enqueue and wait")
    }

    pub async fn get_job(&self, job_id: &str) -> reqwest::Response {
        self.client
            .get(format!("{}/jobs/{job_id}/status", self.base_url))
            .send()
            .await
            .expect("get job")
    }

    pub async fn metrics(&self) -> String {
        self.client
            .get(format!("{}/metrics", self.base_url))
            .send()
            .await
            .expect("metrics")
            .text()
            .await
            .expect("metrics body")
    }

    /// 耗尽 tenant 令牌桶至 denied。
    pub async fn exhaust_tenant(&self, tenant: &str, policy: &str, times: u32) {
        for _ in 0..times {
            let _ = self.check_rate_limit(tenant, policy).await;
        }
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        let redis = self.redis.clone();
        let keys = vec![
            self.state.cfg.stream_key.clone(),
            self.state.cfg.high_priority_stream_key.clone(),
            self.state.cfg.low_priority_stream_key.clone(),
            self.state.cfg.job_dlq_stream.clone(),
            self.state.cfg.callback_retry_stream.clone(),
            self.state.cfg.callback_dlq_stream.clone(),
        ];
        tokio::spawn(async move {
            for key in keys {
                let _: u64 = redis.del(key.as_str()).await.unwrap_or(0);
            }
        });
    }
}

async fn redis_available() -> bool {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
    let Ok(client) = build_redis_client(&url) else {
        return false;
    };
    if client.init().await.is_err() {
        return false;
    }
    let info: Result<String, _> = client.info(Some(InfoKind::Server)).await;
    match info {
        Ok(text) => text.lines().any(|line| {
            line.strip_prefix("redis_version:")
                .and_then(|v| v.split('.').next())
                .and_then(|v| v.parse::<u32>().ok())
                .is_some_and(|major| major >= 7)
        }),
        Err(_) => false,
    }
}

async fn spawn_mock_upstream(delay: Duration) -> (String, JoinHandle<()>) {
    let app = Router::new().fallback({
        let delay = delay;
        move |method: axum::http::Method| {
            let delay = delay;
            async move {
                if method == axum::http::Method::POST {
                    tokio::time::sleep(delay).await;
                    Json(serde_json::json!({ "upstream": true, "model": "test" })).into_response()
                } else {
                    StatusCode::METHOD_NOT_ALLOWED.into_response()
                }
            }
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind upstream");
    let addr = listener.local_addr().expect("upstream addr");
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("upstream serve");
    });
    (format!("http://{addr}"), task)
}

async fn spawn_mock_callback(records: Arc<Mutex<Vec<CallbackRecord>>>) -> (String, JoinHandle<()>) {
    let app = Router::new().route(
        "/cb",
        post({
            let records = records.clone();
            move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                let records = records.clone();
                async move {
                    let job_id = headers
                        .get("x-gateway-job-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    let header_pairs = headers
                        .iter()
                        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                        .collect();
                    records.lock().unwrap_or_else(|e| e.into_inner()).push(CallbackRecord {
                        job_id,
                        body,
                        headers: header_pairs,
                    });
                    StatusCode::OK.into_response()
                }
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind callback");
    let addr = listener.local_addr().expect("callback addr");
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("callback serve");
    });
    (format!("http://{addr}/cb"), task)
}
