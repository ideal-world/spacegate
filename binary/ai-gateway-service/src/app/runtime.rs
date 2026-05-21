pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();

    let args = Args::parse();
    let redis = build_redis_client(&args.redis_url)?;
    let _redis_task = redis.init().await?;
    let worker_redis = build_redis_client(&args.redis_url)?;
    let _worker_redis_task = worker_redis.init().await?;
    let state = AppState {
        redis,
        worker_redis,
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
        .route("/jobs/{job_id}/status", get(get_job))
        .route("/v1/admin/plugins/{plugin}/schema", get(admin_plugin_schema))
        .route("/v1/admin/plugins/{plugin}/readme", get(admin_plugin_readme))
        .route("/v1/admin/tenant-rate-limits", get(admin_list_tenant_rate_limits).put(admin_upsert_tenant_rate_limit).delete(admin_delete_tenant_rate_limit))
        .layer(DefaultBodyLimit::max(args.max_body_bytes))
        .layer(build_admin_cors_layer(&args))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::new(args.host, args.port);
    tracing::info!(%addr, "ai-gateway-service listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
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
