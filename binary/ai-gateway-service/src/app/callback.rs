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

async fn enqueue_callback_retry(state: &AppState, callback_url: &str, job_id: &str, body: &serde_json::Value, last_error: &str) -> Result<(), ServiceError> {
    let body = serde_json::to_string(body).map_err(|e| ServiceError::internal(format!("serialize callback retry: {e}")))?;
    enqueue_callback_retry_raw(
        state,
        callback_url,
        job_id,
        &body,
        1,
        now_ms().saturating_add(state.cfg.callback_retry_initial_delay_ms),
        last_error,
    )
    .await
}

async fn enqueue_callback_retry_raw(
    state: &AppState,
    callback_url: &str,
    job_id: &str,
    body: &str,
    attempt: u32,
    next_attempt_at_ms: u64,
    last_error: &str,
) -> Result<(), ServiceError> {
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
                ("body", Value::String(body.to_string().into())),
                ("attempt", Value::Integer(attempt as i64)),
                ("next_attempt_at_ms", Value::Integer(next_attempt_at_ms as i64)),
                ("last_error", Value::String(last_error.to_string().into())),
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
    reclaim_callback_retries(state).await?;
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
            process_callback_retry_entry(state, entry_id.as_str(), &fields).await?;
        }
    }
    Ok(())
}

async fn reclaim_callback_retries(state: &AppState) -> Result<(), ServiceError> {
    let consumer = format!("{}-callback-reclaimer", state.cfg.consumer_name);
    let min_idle_ms = state.cfg.callback_retry_reclaim_idle_secs.saturating_mul(1000);
    let (_cursor, entries): (String, Vec<(String, HashMap<String, Value>)>) = state
        .redis
        .xautoclaim_values(
            state.cfg.callback_retry_stream.as_str(),
            state.cfg.callback_retry_group.as_str(),
            consumer.as_str(),
            min_idle_ms,
            "0-0",
            Some(10),
            false,
        )
        .await?;
    for (entry_id, fields) in entries {
        process_callback_retry_entry(state, entry_id.as_str(), &fields).await?;
    }
    Ok(())
}

async fn process_callback_retry_entry(state: &AppState, entry_id: &str, fields: &HashMap<String, Value>) -> Result<(), ServiceError> {
    let job_id = field_string(fields, "job_id").unwrap_or_default();
    let callback_url = field_string(fields, "callback_url").unwrap_or_default();
    let body = field_string(fields, "body").unwrap_or_else(|| "{}".to_string());
    let attempt = field_u32(fields, "attempt").unwrap_or(1);
    let next_attempt_at_ms = field_u64(fields, "next_attempt_at_ms").unwrap_or(0);
    let now = now_ms();

    if next_attempt_at_ms > now {
        let last_error = field_string(fields, "last_error").unwrap_or_default();
        enqueue_callback_retry_raw(state, &callback_url, &job_id, &body, attempt, next_attempt_at_ms, &last_error).await?;
        ack_callback_retry(state, entry_id).await?;
        return Ok(());
    }

    let parsed = serde_json::from_str::<serde_json::Value>(&body).unwrap_or_else(|_| serde_json::json!({ "body": body }));
    match post_callback(state, &callback_url, &job_id, &parsed).await {
        Ok(()) => {
            ack_callback_retry(state, entry_id).await?;
            state.metrics.callback_retry_success_total.fetch_add(1, Ordering::Relaxed);
        }
        Err(e) => {
            tracing::warn!(job_id = %job_id, attempt, error = %e.message, "callback retry failed");
            if attempt >= state.cfg.callback_max_retry_attempts {
                enqueue_callback_dlq(state, &callback_url, &job_id, &parsed, attempt, &e.message).await?;
                ack_callback_retry(state, entry_id).await?;
            } else {
                let next_attempt = attempt.saturating_add(1);
                let delay_ms = callback_retry_delay_ms(state.cfg.callback_retry_initial_delay_ms, state.cfg.callback_retry_max_delay_ms, next_attempt);
                let retry_body = serde_json::to_string(&parsed).unwrap_or_else(|_| "{}".to_string());
                enqueue_callback_retry_raw(state, &callback_url, &job_id, &retry_body, next_attempt, now.saturating_add(delay_ms), &e.message).await?;
                ack_callback_retry(state, entry_id).await?;
            }
        }
    }
    Ok(())
}

async fn ack_callback_retry(state: &AppState, entry_id: &str) -> Result<(), ServiceError> {
    let _: i64 = state.redis.xack(state.cfg.callback_retry_stream.as_str(), state.cfg.callback_retry_group.as_str(), vec![entry_id]).await?;
    Ok(())
}

async fn enqueue_callback_dlq(state: &AppState, callback_url: &str, job_id: &str, body: &serde_json::Value, attempts: u32, final_error: &str) -> Result<(), ServiceError> {
    let body = serde_json::to_string(body).map_err(|e| ServiceError::internal(format!("serialize callback dlq: {e}")))?;
    let _: String = state
        .redis
        .xadd(
            state.cfg.callback_dlq_stream.as_str(),
            false,
            None::<()>,
            "*",
            vec![
                ("job_id", Value::String(job_id.to_string().into())),
                ("callback_url", Value::String(callback_url.to_string().into())),
                ("body", Value::String(body.into())),
                ("attempts", Value::Integer(attempts as i64)),
                ("final_error", Value::String(final_error.to_string().into())),
                ("failed_at", Value::Integer(now_ms() as i64)),
            ],
        )
        .await?;
    trim_stream(state, &state.cfg.callback_dlq_stream).await?;
    state.metrics.callback_retry_dlq_total.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

fn callback_retry_delay_ms(initial_delay_ms: u64, max_delay_ms: u64, attempt: u32) -> u64 {
    let exponent = attempt.saturating_sub(1).min(16);
    let multiplier = 1u64.checked_shl(exponent).unwrap_or(u64::MAX);
    initial_delay_ms.saturating_mul(multiplier).min(max_delay_ms)
}
