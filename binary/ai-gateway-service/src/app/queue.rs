async fn enqueue_job(state: &AppState, policy: QueuePolicy, _method: Method, uri: Uri, headers: HeaderMap, body: Body) -> Result<AcceptedJob, ServiceError> {
    let enqueue_started_at = now_ms();
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
    let body_outcome = store_body(state, &job_id, body).await?;
    let body_ref = body_outcome.location;
    let body_size = body_ref.size;
    let body_storage = body_ref.storage;
    let (stream_key, priority) = stream_for_request(state, &headers, &tenant_id, &model);

    let xadd_future = async {
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
                    ("priority", Value::String(priority.as_str().into())),
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
        Ok::<String, ServiceError>(stream_id)
    };

    let stream_id = if let Some(upload) = body_outcome.pending_upload {
        let (upload_join, stream_id_result) = tokio::join!(upload, xadd_future);
        let upload_result = upload_join.map_err(|e| ServiceError::internal(format!("body upload task failed: {e}")))?;
        upload_result?;
        stream_id_result?
    } else {
        xadd_future.await?
    };

    state.metrics.enqueue_total.fetch_add(1, Ordering::Relaxed);
    observe_enqueue_latency(
        &state.metrics,
        now_ms().saturating_sub(enqueue_started_at),
        policy.as_str(),
        body_size_bucket(body_size, body_storage),
    );
    observe_body_size(&state.metrics, body_size);
    match priority {
        QueuePriority::High => {
            state.metrics.enqueue_priority_high_total.fetch_add(1, Ordering::Relaxed);
        }
        QueuePriority::Normal => {
            state.metrics.enqueue_priority_normal_total.fetch_add(1, Ordering::Relaxed);
        }
        QueuePriority::Low => {
            state.metrics.enqueue_priority_low_total.fetch_add(1, Ordering::Relaxed);
        }
    }
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
            poll_url: job_poll_url(&job_id),
            status_url: job_status_url_legacy(&job_id),
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
    let streams = worker_stream_order(state);
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
    let reply = xreadgroup_map_or_empty(
        &state.worker_redis,
        state.cfg.consumer_group.as_str(),
        consumer,
        Some(5),
        Some(block_ms),
        false,
        vec![stream],
        vec![">"],
    )
    .await?;

    let mut tasks = Vec::new();
    for (_stream, entries) in reply {
        for (entry_id, fields) in entries {
            let state = state.clone();
            let stream = stream.to_string();
            tasks.push(tokio::spawn(async move {
                process_stream_entry(&state, stream.as_str(), entry_id.as_str(), &fields).await
            }));
        }
    }

    let mut processed = 0;
    for task in tasks {
        match task.await {
            Ok(Ok(true)) => processed += 1,
            Ok(Ok(false)) => {}
            Ok(Err(e)) => {
                tracing::warn!(error = %e.message, "job processing failed");
                state.metrics.worker_failed_total.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                tracing::warn!(error = %e, "worker task join failed");
                state.metrics.worker_failed_total.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    Ok(processed)
}

async fn process_stream_entry(state: &AppState, stream: &str, entry_id: &str, fields: &HashMap<String, Value>) -> Result<bool, ServiceError> {
    let job_id = field_string(fields, "job_id").ok_or_else(|| ServiceError::bad_request("job missing job_id"))?;
    let lease_owner = format!("{}:{stream}:{entry_id}:{}", state.cfg.consumer_name, now_ms());

    if !acquire_job_lease(state, &job_id, &lease_owner).await? {
        state.metrics.lease_skip_total.fetch_add(1, Ordering::Relaxed);
        tracing::info!(job_id = %job_id, stream = %stream, entry_id = %entry_id, "job is already leased; skip reclaimed duplicate");
        return Ok(false);
    }

    let attempt = increment_job_delivery_attempt(state, &job_id).await?;
    if attempt > state.cfg.job_max_delivery_attempts {
        enqueue_job_dlq(state, stream, entry_id, fields, attempt, "max_delivery_attempts_exceeded").await?;
        ack_stream_entry(state, stream, entry_id).await?;
        release_job_lease(state, &job_id).await;
        state.metrics.job_dlq_total.fetch_add(1, Ordering::Relaxed);
        return Ok(true);
    }

    let processing_started_at = now_ms();
    let model = field_string(fields, "model").unwrap_or_else(|| "default".to_string());
    match process_job(state, stream, entry_id, fields).await {
        Ok(()) => {
            observe_worker_processing(&state.metrics, now_ms().saturating_sub(processing_started_at), &model);
            ack_stream_entry(state, stream, entry_id).await?;
            clear_job_delivery_attempt(state, &job_id).await;
            release_job_lease(state, &job_id).await;
            Ok(true)
        }
        Err(e) => {
            observe_worker_processing(&state.metrics, now_ms().saturating_sub(processing_started_at), &model);
            release_job_lease(state, &job_id).await;
            Err(e)
        }
    }
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
            enqueue_callback_retry(state, &callback_url, &job_id, &callback_body, e.message.as_str()).await?;
        }
    }
    state.metrics.worker_completed_total.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

async fn acquire_job_lease(state: &AppState, job_id: &str, owner: &str) -> Result<bool, ServiceError> {
    let key = job_lease_key(job_id);
    let result: Option<String> = state
        .redis
        .set(
            key,
            owner,
            Some(Expiration::EX(state.cfg.job_process_lease_secs.max(1) as i64)),
            Some(SetOptions::NX),
            false,
        )
        .await?;
    Ok(result.is_some())
}

async fn release_job_lease(state: &AppState, job_id: &str) {
    let _: Result<i64, _> = state.redis.del(job_lease_key(job_id)).await;
}

async fn increment_job_delivery_attempt(state: &AppState, job_id: &str) -> Result<u32, ServiceError> {
    let key = job_attempt_key(job_id);
    let attempt: i64 = state.redis.incr_by(key.as_str(), 1).await?;
    let _: () = state.redis.expire(key.as_str(), state.cfg.result_ttl_secs.max(300) as i64, None::<ExpireOptions>).await?;
    Ok(attempt.max(0) as u32)
}

async fn clear_job_delivery_attempt(state: &AppState, job_id: &str) {
    let _: Result<i64, _> = state.redis.del(job_attempt_key(job_id)).await;
}

async fn ack_stream_entry(state: &AppState, stream: &str, entry_id: &str) -> Result<(), ServiceError> {
    let _: i64 = state.redis.xack(stream, state.cfg.consumer_group.as_str(), vec![entry_id]).await?;
    Ok(())
}

async fn enqueue_job_dlq(state: &AppState, stream: &str, entry_id: &str, fields: &HashMap<String, Value>, attempts: u32, reason: &str) -> Result<(), ServiceError> {
    let job_id = field_string(fields, "job_id").unwrap_or_default();
    let fields_json = stream_fields_to_json(fields)?;
    let _: String = state
        .redis
        .xadd(
            state.cfg.job_dlq_stream.as_str(),
            false,
            None::<()>,
            "*",
            vec![
                ("job_id", Value::String(job_id.into())),
                ("source_stream", Value::String(stream.to_string().into())),
                ("source_entry_id", Value::String(entry_id.to_string().into())),
                ("attempts", Value::Integer(attempts as i64)),
                ("reason", Value::String(reason.to_string().into())),
                ("fields", Value::String(fields_json.into())),
                ("failed_at", Value::Integer(now_ms() as i64)),
            ],
        )
        .await?;
    trim_stream(state, &state.cfg.job_dlq_stream).await?;
    Ok(())
}

fn stream_fields_to_json(fields: &HashMap<String, Value>) -> Result<String, ServiceError> {
    let mut out = HashMap::new();
    for (key, value) in fields {
        if let Some(value) = field_string(fields, key) {
            out.insert(key.clone(), value);
        } else {
            out.insert(key.clone(), format!("{value:?}"));
        }
    }
    serde_json::to_string(&out).map_err(|e| ServiceError::internal(format!("serialize job dlq fields: {e}")))
}

fn job_lease_key(job_id: &str) -> String {
    format!("ai:job:lease:{}", sanitize_key(job_id))
}

fn job_attempt_key(job_id: &str) -> String {
    format!("ai:job:attempt:{}", sanitize_key(job_id))
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
    for stream in configured_streams(state) {
        let (_cursor, entries): (String, Vec<(String, HashMap<String, Value>)>) =
            state.worker_redis.xautoclaim_values(stream.as_str(), state.cfg.consumer_group.as_str(), consumer.as_str(), min_idle_ms, "0-0", Some(10), false).await?;
        for (entry_id, fields) in entries {
            match process_stream_entry(state, stream.as_str(), entry_id.as_str(), &fields).await {
                Ok(true) => {
                    state.metrics.reclaimed_total.fetch_add(1, Ordering::Relaxed);
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(stream = %stream, entry_id = %entry_id, error = %e.message, "reclaimed job failed");
                }
            }
        }
    }
    Ok(())
}

async fn ensure_consumer_groups(state: &AppState) -> Result<(), ServiceError> {
    for stream in configured_streams(state) {
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

fn stream_for_request(state: &AppState, headers: &HeaderMap, tenant: &str, model: &str) -> (String, QueuePriority) {
    let priority = request_priority(state, headers, tenant, model);
    if !state.cfg.enable_priority_streams {
        return (state.cfg.stream_key.clone(), priority);
    }
    let stream = match priority {
        QueuePriority::High => state.cfg.high_priority_stream_key.clone(),
        QueuePriority::Low => state.cfg.low_priority_stream_key.clone(),
        QueuePriority::Normal => state.cfg.stream_key.clone(),
    };
    (stream, priority)
}

fn request_priority(state: &AppState, headers: &HeaderMap, tenant: &str, model: &str) -> QueuePriority {
    if let Some(priority) = optional_header(headers, "x-queue-priority").and_then(|value| parse_queue_priority(&value)) {
        return priority;
    }
    if contains_csv_value(&state.cfg.queue_high_tenants, tenant) || contains_csv_value(&state.cfg.queue_high_models, model) {
        return QueuePriority::High;
    }
    if contains_csv_value(&state.cfg.queue_low_tenants, tenant) || contains_csv_value(&state.cfg.queue_low_models, model) {
        return QueuePriority::Low;
    }
    parse_queue_priority(&state.cfg.queue_default_priority).unwrap_or(QueuePriority::Normal)
}

fn configured_streams(state: &AppState) -> Vec<String> {
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

fn worker_stream_order(state: &AppState) -> Vec<String> {
    if !state.cfg.enable_priority_streams {
        return vec![state.cfg.stream_key.clone()];
    }
    let mut out = Vec::new();
    push_weighted(&mut out, &state.cfg.high_priority_stream_key, state.cfg.queue_high_weight);
    push_weighted(&mut out, &state.cfg.stream_key, state.cfg.queue_normal_weight);
    push_weighted(&mut out, &state.cfg.low_priority_stream_key, state.cfg.queue_low_weight);
    if out.is_empty() {
        out.push(state.cfg.stream_key.clone());
    }
    out
}

fn push_weighted(out: &mut Vec<String>, stream: &str, weight: usize) {
    for _ in 0..weight {
        out.push(stream.to_string());
    }
}

fn parse_queue_priority(value: &str) -> Option<QueuePriority> {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => Some(QueuePriority::High),
        "normal" | "default" | "medium" => Some(QueuePriority::Normal),
        "low" => Some(QueuePriority::Low),
        _ => None,
    }
}

fn contains_csv_value(csv: &str, needle: &str) -> bool {
    csv.split(',').map(str::trim).filter(|value| !value.is_empty()).any(|value| value.eq_ignore_ascii_case(needle))
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
