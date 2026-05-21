async fn healthz() -> &'static str {
    "ok"
}

async fn check_rate_limit(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Result<Json<RateLimitResponse>, ServiceError> {
    let tenant = required_header(&headers, "x-tenant-id")?;
    let model = optional_header(&headers, "x-model").unwrap_or_else(|| "default".to_string());
    let path = optional_header(&headers, "x-original-path").unwrap_or_else(|| uri.path().to_string());
    let policy = optional_header(&headers, "x-ratelimit-policy").unwrap_or_else(|| "abandon".to_string());
    let rate_limit = tenant_rate_limit(&state, &tenant, &model, &path, &policy).await?;
    let key = sanitize_key(&format!("{tenant}:{model}:{path}"));
    let tokens_key = format!("ai:ratelimit:{key}:tokens");
    let ts_key = format!("ai:ratelimit:{key}:ts");
    let now = now_ms();

    let out: Vec<i64> = state
        .redis
        .eval(
            TOKEN_BUCKET_LUA,
            vec![tokens_key, ts_key],
            vec![rate_limit.rps.to_string(), rate_limit.burst.to_string(), now.to_string(), rate_limit.cost.to_string()],
        )
        .await?;

    let allowed = out.first().copied().unwrap_or(0) == 1;
    if !allowed {
        state.metrics.rate_limited_total.fetch_add(1, Ordering::Relaxed);
        inc_labeled(
            &state.metrics,
            format!(
                r#"rate_limited_total{{policy="{}",tenant="{}"}}"#,
                metrics_label(&policy),
                metrics_label(&tenant)
            ),
        );
    }
    Ok(Json(RateLimitResponse {
        allowed,
        remaining_tokens_milli: out.get(1).copied().unwrap_or(0),
        retry_after_ms: out.get(2).copied().unwrap_or(0),
    }))
}

async fn enqueue(State(state): State<AppState>, method: Method, uri: Uri, headers: HeaderMap, body: Body) -> Result<impl IntoResponse, ServiceError> {
    let accepted = enqueue_job(&state, QueuePolicy::Queue, method, uri, headers, body).await?;
    let mut resp = (StatusCode::ACCEPTED, Json(&accepted.response)).into_response();
    resp.headers_mut().insert("x-job-id", header_value(&accepted.response.job_id)?);
    resp.headers_mut().insert("location", header_value(&accepted.response.poll_url)?);
    Ok(resp)
}

async fn enqueue_and_wait(State(state): State<AppState>, method: Method, uri: Uri, headers: HeaderMap, body: Body) -> Result<Response, ServiceError> {
    let timeout_secs = optional_header(&headers, "x-request-timeout").and_then(|v| v.parse::<u64>().ok()).unwrap_or(state.cfg.wait_timeout_secs);
    state.metrics.wait_total.fetch_add(1, Ordering::Relaxed);
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
    let job_dlq_depth: i64 = state.redis.xlen(state.cfg.job_dlq_stream.as_str()).await.unwrap_or_default();
    let callback_retry_depth: i64 = state.redis.xlen(state.cfg.callback_retry_stream.as_str()).await.unwrap_or_default();
    let callback_dlq_depth: i64 = state.redis.xlen(state.cfg.callback_dlq_stream.as_str()).await.unwrap_or_default();
    let pel_size = pending_size(&state, &state.cfg.stream_key).await;
    let high_pel_size = pending_size(&state, &state.cfg.high_priority_stream_key).await;
    let low_pel_size = pending_size(&state, &state.cfg.low_priority_stream_key).await;
    let callback_retry_pel_size = pending_size_for_group(&state, &state.cfg.callback_retry_stream, &state.cfg.callback_retry_group).await;

    let wait_total = state.metrics.wait_total.load(Ordering::Relaxed);
    let wait_timeout_total = state.metrics.wait_timeout_total.load(Ordering::Relaxed);
    let callback_failure_total = state.metrics.callback_failure_total.load(Ordering::Relaxed);
    let worker_completed_total = state.metrics.worker_completed_total.load(Ordering::Relaxed);
    let wait_timeout_rate = if wait_total > 0 {
        wait_timeout_total as f64 / wait_total as f64
    } else {
        0.0
    };
    let callback_failure_rate = if worker_completed_total > 0 {
        callback_failure_total as f64 / worker_completed_total as f64
    } else {
        0.0
    };
    let labeled_lines = format_labeled_lines(&state.metrics);

    let body = format!(
        "\
rate_limited_total {}\n\
enqueue_total {}\n\
enqueue_total{{policy=\"queue\"}} {}\n\
enqueue_total{{policy=\"wait\"}} {}\n\
enqueue_total{{priority=\"high\"}} {}\n\
enqueue_total{{priority=\"normal\"}} {}\n\
enqueue_total{{priority=\"low\"}} {}\n\
enqueue_latency_ms_count {}\n\
enqueue_latency_ms_sum {}\n\
enqueue_latency_ms_bucket{{le=\"100\"}} {}\n\
enqueue_latency_ms_bucket{{le=\"500\"}} {}\n\
enqueue_latency_ms_bucket{{le=\"1000\"}} {}\n\
enqueue_latency_ms_bucket{{le=\"+Inf\"}} {}\n\
enqueue_body_size_bytes_count {}\n\
enqueue_body_size_bytes_sum {}\n\
enqueue_body_size_bytes_bucket{{le=\"10240\"}} {}\n\
enqueue_body_size_bytes_bucket{{le=\"131072\"}} {}\n\
enqueue_body_size_bytes_bucket{{le=\"5242880\"}} {}\n\
enqueue_body_size_bytes_bucket{{le=\"+Inf\"}} {}\n\
wait_total {}\n\
wait_timeout_total {}\n\
wait_timeout_rate {:.6}\n\
callback_failure_total {}\n\
callback_failure_rate {:.6}\n\
callback_retry_total {}\n\
callback_retry_success_total {}\n\
callback_retry_dlq_total {}\n\
worker_completed_total {}\n\
worker_failed_total {}\n\
worker_processing_time_ms_count {}\n\
worker_processing_time_ms_sum {}\n\
worker_processing_time_ms_bucket{{le=\"1000\"}} {}\n\
worker_processing_time_ms_bucket{{le=\"5000\"}} {}\n\
worker_processing_time_ms_bucket{{le=\"30000\"}} {}\n\
worker_processing_time_ms_bucket{{le=\"+Inf\"}} {}\n\
reclaimed_total {}\n\
job_dlq_total {}\n\
lease_skip_total {}\n\
object_offload_total {}\n\
object_multipart_abort_total {}\n\
queue_depth {}\n\
queue_depth{{priority=\"normal\"}} {}\n\
queue_depth{{priority=\"high\"}} {}\n\
queue_depth{{priority=\"low\"}} {}\n\
pel_size {}\n\
pel_size{{priority=\"normal\"}} {}\n\
pel_size{{priority=\"high\"}} {}\n\
pel_size{{priority=\"low\"}} {}\n\
job_dlq_depth {}\n\
callback_retry_depth {}\n\
callback_retry_pel_size {}\n\
callback_dlq_depth {}\n\
{labeled_lines}\n",
        state.metrics.rate_limited_total.load(Ordering::Relaxed),
        state.metrics.enqueue_total.load(Ordering::Relaxed),
        state.metrics.enqueue_queue_total.load(Ordering::Relaxed),
        state.metrics.enqueue_wait_total.load(Ordering::Relaxed),
        state.metrics.enqueue_priority_high_total.load(Ordering::Relaxed),
        state.metrics.enqueue_priority_normal_total.load(Ordering::Relaxed),
        state.metrics.enqueue_priority_low_total.load(Ordering::Relaxed),
        state.metrics.enqueue_latency_count.load(Ordering::Relaxed),
        state.metrics.enqueue_latency_sum_ms.load(Ordering::Relaxed),
        state.metrics.enqueue_latency_le_100_ms.load(Ordering::Relaxed),
        state.metrics.enqueue_latency_le_100_ms.load(Ordering::Relaxed) + state.metrics.enqueue_latency_le_500_ms.load(Ordering::Relaxed),
        state.metrics.enqueue_latency_le_100_ms.load(Ordering::Relaxed)
            + state.metrics.enqueue_latency_le_500_ms.load(Ordering::Relaxed)
            + state.metrics.enqueue_latency_le_1000_ms.load(Ordering::Relaxed),
        state.metrics.enqueue_latency_count.load(Ordering::Relaxed),
        state.metrics.body_size_count.load(Ordering::Relaxed),
        state.metrics.body_size_sum_bytes.load(Ordering::Relaxed),
        state.metrics.body_size_le_10kb.load(Ordering::Relaxed),
        state.metrics.body_size_le_10kb.load(Ordering::Relaxed) + state.metrics.body_size_le_128kb.load(Ordering::Relaxed),
        state.metrics.body_size_le_10kb.load(Ordering::Relaxed) + state.metrics.body_size_le_128kb.load(Ordering::Relaxed) + state.metrics.body_size_le_5mb.load(Ordering::Relaxed),
        state.metrics.body_size_count.load(Ordering::Relaxed),
        state.metrics.wait_total.load(Ordering::Relaxed),
        state.metrics.wait_timeout_total.load(Ordering::Relaxed),
        wait_timeout_rate,
        state.metrics.callback_failure_total.load(Ordering::Relaxed),
        callback_failure_rate,
        state.metrics.callback_retry_total.load(Ordering::Relaxed),
        state.metrics.callback_retry_success_total.load(Ordering::Relaxed),
        state.metrics.callback_retry_dlq_total.load(Ordering::Relaxed),
        state.metrics.worker_completed_total.load(Ordering::Relaxed),
        state.metrics.worker_failed_total.load(Ordering::Relaxed),
        state.metrics.worker_processing_count.load(Ordering::Relaxed),
        state.metrics.worker_processing_sum_ms.load(Ordering::Relaxed),
        state.metrics.worker_processing_le_1000_ms.load(Ordering::Relaxed),
        state.metrics.worker_processing_le_1000_ms.load(Ordering::Relaxed) + state.metrics.worker_processing_le_5000_ms.load(Ordering::Relaxed),
        state.metrics.worker_processing_le_1000_ms.load(Ordering::Relaxed)
            + state.metrics.worker_processing_le_5000_ms.load(Ordering::Relaxed)
            + state.metrics.worker_processing_le_30000_ms.load(Ordering::Relaxed),
        state.metrics.worker_processing_count.load(Ordering::Relaxed),
        state.metrics.reclaimed_total.load(Ordering::Relaxed),
        state.metrics.job_dlq_total.load(Ordering::Relaxed),
        state.metrics.lease_skip_total.load(Ordering::Relaxed),
        state.metrics.object_offload_total.load(Ordering::Relaxed),
        state.metrics.object_multipart_abort_total.load(Ordering::Relaxed),
        queue_depth,
        queue_depth,
        high_queue_depth,
        low_queue_depth,
        pel_size,
        pel_size,
        high_pel_size,
        low_pel_size,
        job_dlq_depth,
        callback_retry_depth,
        callback_retry_pel_size,
        callback_dlq_depth,
    );
    Ok((StatusCode::OK, [("content-type", "text/plain; version=0.0.4")], body).into_response())
}
