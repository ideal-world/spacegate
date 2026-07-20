pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    let args = load_args()?;
    let redis = build_redis_client(&args.redis_url)?;
    let _redis_task = redis.init().await?;
    check_redis_version(&redis).await?;
    let worker_redis = build_redis_client(&args.redis_url)?;
    let _worker_redis_task = worker_redis.init().await?;
    let wait_subscriber = WaitSubscriberHub::new(&args.redis_url).await?;
    let state = AppState {
        redis,
        worker_redis,
        http: reqwest::Client::new(),
        cfg: Arc::new(args.clone()),
        body_permits: Arc::new(Semaphore::new(args.body_read_concurrency.max(1))),
        metrics: Arc::new(Metrics::default()),
        wait_subscriber,
    };

    ensure_consumer_groups(&state).await?;
    if state.cfg.upstream_base_url.is_some() {
        spawn_workers(state.clone());
        spawn_reclaimer(state.clone());
        spawn_callback_retry_worker(state.clone());
    } else {
        tracing::warn!("AI_UPSTREAM_BASE_URL is not set; queue jobs will be stored but no local worker will process them");
    }

    let app = build_router(state, args.max_body_bytes);

    let addr = SocketAddr::new(args.host, args.port);
    tracing::info!(%addr, "ai-gateway-service listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// 构建 HTTP 路由，供 main 与集成测试复用。
pub fn build_router(state: AppState, max_body_bytes: usize) -> Router {
    // Health probes are intentionally excluded to keep business request logs readable.
    let business_routes = Router::new()
        .route("/v1/ratelimit/check", post(check_rate_limit))
        .route("/v1/queue/enqueue", post(enqueue))
        .route("/v1/queue/enqueue-and-wait", post(enqueue_and_wait))
        .route("/v1/jobs/{job_id}", get(get_job))
        .route("/jobs/{job_id}/status", get(get_job))
        .route("/v1/admin/plugins/{plugin}/schema", get(admin_plugin_schema))
        .route("/v1/admin/plugins/{plugin}/readme", get(admin_plugin_readme))
        .route("/v1/admin/tenant-rate-limits/resolve", get(admin_resolve_tenant_rate_limit))
        .route(
            "/v1/admin/tenant-rate-limits",
            get(admin_list_tenant_rate_limits).put(admin_upsert_tenant_rate_limit).delete(admin_delete_tenant_rate_limit),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| tracing::info_span!("http_request", method = %request.method(), path = request_log_path(request.uri())))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );

    Router::new()
        .route("/healthz", get(healthz))
        .route("/metrics", get(metrics))
        .merge(business_routes)
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .layer(build_admin_cors_layer(state.cfg.as_ref()))
        .with_state(state)
}

async fn check_redis_version(redis: &FredClient) -> Result<(), Box<dyn std::error::Error>> {
    let info: String = redis.info(Some(InfoKind::Server)).await?;
    for line in info.lines() {
        if let Some(version) = line.strip_prefix("redis_version:") {
            let major = version.split('.').next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            if major < 7 {
                return Err(format!("Redis 7+ is required, found redis_version={version}").into());
            }
            tracing::info!(redis_version = %version.trim(), "redis version check passed");
            return Ok(());
        }
    }
    tracing::warn!("could not parse redis_version from INFO; continuing without version check");
    Ok(())
}

fn build_admin_cors_layer(args: &Args) -> CorsLayer {
    let origins: Vec<HeaderValue> = args
        .admin_cors_origins
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| HeaderValue::from_str(value).ok())
        .collect();
    if origins.is_empty() {
        return CorsLayer::permissive();
    }
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
}
