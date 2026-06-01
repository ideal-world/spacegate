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
        if !should_return_upstream_header(&name) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(name.as_str()), HeaderValue::from_str(&value)) {
            resp.headers_mut().insert(name, value);
        }
    }
    resp.headers_mut().insert("x-job-id", header_value(&result.job_id)?);
    resp.headers_mut().insert("x-queue-wait-ms", header_value(&now_ms().saturating_sub(created_at_ms).to_string())?);
    Ok(resp)
}
