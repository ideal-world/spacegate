async fn trim_stream(state: &AppState, stream: &str) -> Result<(), ServiceError> {
    if state.cfg.stream_max_len > 0 {
        let _: i64 = state.redis.xtrim(stream, ("MAXLEN", "~", state.cfg.stream_max_len as i64)).await?;
    }
    Ok(())
}

async fn pending_size(state: &AppState, stream: &str) -> i64 {
    pending_size_for_group(state, stream, state.cfg.consumer_group.as_str()).await
}

async fn pending_size_for_group(state: &AppState, stream: &str, group: &str) -> i64 {
    let raw: FredResult<Value> = state.redis.xpending(stream, group, ()).await;
    match raw {
        Ok(value) => pending_count_from_value(&value),
        Err(e) => {
            tracing::debug!(stream = %stream, group = %group, error = %e, "read stream pending size failed");
            0
        }
    }
}

fn pending_count_from_value(value: &Value) -> i64 {
    match value {
        Value::Integer(value) => (*value).max(0),
        Value::String(value) => value.parse::<i64>().unwrap_or(0).max(0),
        Value::Bytes(value) => std::str::from_utf8(value).ok().and_then(|value| value.parse::<i64>().ok()).unwrap_or(0).max(0),
        Value::Array(values) => values.first().map(pending_count_from_value).unwrap_or(0),
        Value::Map(values) => values
            .iter()
            .find_map(|(key, value)| {
                let key = key.as_str()?;
                if key.eq_ignore_ascii_case("pending") || key.eq_ignore_ascii_case("count") {
                    Some(pending_count_from_value(value))
                } else {
                    None
                }
            })
            .unwrap_or(0),
        _ => 0,
    }
}

fn inc_labeled(metrics: &Metrics, key: impl Into<String>) {
    let mut map = metrics.labeled.lock().unwrap_or_else(|error| error.into_inner());
    *map.entry(key.into()).or_insert(0) += 1;
}

fn format_labeled_lines(metrics: &Metrics) -> String {
    let map = metrics.labeled.lock().unwrap_or_else(|error| error.into_inner());
    let mut keys: Vec<_> = map.keys().cloned().collect();
    keys.sort();
    keys.into_iter()
        .filter_map(|key| map.get(&key).copied().map(|value| format!("{key} {value}")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn observe_enqueue_latency(metrics: &Metrics, elapsed_ms: u64, policy: &str, size_bucket: &str) {
    metrics.enqueue_latency_count.fetch_add(1, Ordering::Relaxed);
    metrics.enqueue_latency_sum_ms.fetch_add(elapsed_ms, Ordering::Relaxed);
    let le = if elapsed_ms <= 100 {
        metrics.enqueue_latency_le_100_ms.fetch_add(1, Ordering::Relaxed);
        "100"
    } else if elapsed_ms <= 500 {
        metrics.enqueue_latency_le_500_ms.fetch_add(1, Ordering::Relaxed);
        "500"
    } else if elapsed_ms <= 1000 {
        metrics.enqueue_latency_le_1000_ms.fetch_add(1, Ordering::Relaxed);
        "1000"
    } else {
        metrics.enqueue_latency_gt_1000_ms.fetch_add(1, Ordering::Relaxed);
        "+Inf"
    };
    inc_labeled(
        metrics,
        format!(r#"enqueue_latency_ms_bucket{{policy="{policy}",size_bucket="{size_bucket}",le="{le}"}}"#),
    );
}

fn observe_body_size(metrics: &Metrics, size: usize) {
    metrics.body_size_count.fetch_add(1, Ordering::Relaxed);
    metrics.body_size_sum_bytes.fetch_add(size as u64, Ordering::Relaxed);
    if size <= 10 * 1024 {
        metrics.body_size_le_10kb.fetch_add(1, Ordering::Relaxed);
    } else if size <= 128 * 1024 {
        metrics.body_size_le_128kb.fetch_add(1, Ordering::Relaxed);
    } else if size <= 5 * 1024 * 1024 {
        metrics.body_size_le_5mb.fetch_add(1, Ordering::Relaxed);
    } else {
        metrics.body_size_gt_5mb.fetch_add(1, Ordering::Relaxed);
    }
}

fn observe_worker_processing(metrics: &Metrics, elapsed_ms: u64, model: &str) {
    metrics.worker_processing_count.fetch_add(1, Ordering::Relaxed);
    metrics.worker_processing_sum_ms.fetch_add(elapsed_ms, Ordering::Relaxed);
    let model = metrics_label(model);
    let le = if elapsed_ms <= 1000 {
        metrics.worker_processing_le_1000_ms.fetch_add(1, Ordering::Relaxed);
        "1000"
    } else if elapsed_ms <= 5000 {
        metrics.worker_processing_le_5000_ms.fetch_add(1, Ordering::Relaxed);
        "5000"
    } else if elapsed_ms <= 30_000 {
        metrics.worker_processing_le_30000_ms.fetch_add(1, Ordering::Relaxed);
        "30000"
    } else {
        metrics.worker_processing_gt_30000_ms.fetch_add(1, Ordering::Relaxed);
        "+Inf"
    };
    inc_labeled(
        metrics,
        format!(r#"worker_processing_time_ms_bucket{{model="{model}",le="{le}"}}"#),
    );
}
