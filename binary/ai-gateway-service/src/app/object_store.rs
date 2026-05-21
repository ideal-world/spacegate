async fn store_body(state: &AppState, job_id: &str, body: Body) -> Result<BodyLocation, ServiceError> {
    let object_ref = format!("{}/{}/body.bin", state.cfg.object_store_prefix.trim_matches('/'), sanitize_key(job_id));
    let mut stream = body.into_data_stream();
    let mut pending = Vec::new();
    let mut total_size = 0usize;
    let mut upload_id = None;
    let mut parts = Vec::new();
    let part_size = state.cfg.object_multipart_part_size.max(5 * 1024 * 1024);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ServiceError::bad_request(format!("read request body: {e}")))?;
        total_size = total_size.checked_add(chunk.len()).ok_or_else(|| ServiceError::payload_too_large("request body is too large"))?;
        if total_size > state.cfg.max_body_bytes {
            abort_upload_if_needed(state, &object_ref, upload_id.as_deref()).await;
            return Err(ServiceError::payload_too_large(format!("request body exceeds max size {}", state.cfg.max_body_bytes)));
        }

        if upload_id.is_none() {
            if state.cfg.object_store_endpoint.is_some() && pending.len() + chunk.len() > state.cfg.inline_threshold {
                pending.extend_from_slice(&chunk);
                match initiate_multipart_upload(state, &object_ref).await {
                    Ok(id) => upload_id = Some(id),
                    Err(e) => return Err(e),
                }
            } else {
                pending.extend_from_slice(&chunk);
                continue;
            }
        } else {
            pending.extend_from_slice(&chunk);
        }

        if let Some(upload_id) = upload_id.as_deref() {
            while pending.len() >= part_size {
                let part_body = pending.drain(..part_size).collect::<Vec<_>>();
                match upload_multipart_part(state, &object_ref, upload_id, parts.len() + 1, part_body).await {
                    Ok(part) => parts.push(part),
                    Err(e) => {
                        abort_upload_if_needed(state, &object_ref, Some(upload_id)).await;
                        return Err(e);
                    }
                }
            }
        }
    }

    if let Some(upload_id) = upload_id.as_deref() {
        if !pending.is_empty() || parts.is_empty() {
            match upload_multipart_part(state, &object_ref, upload_id, parts.len() + 1, pending).await {
                Ok(part) => parts.push(part),
                Err(e) => {
                    abort_upload_if_needed(state, &object_ref, Some(upload_id)).await;
                    return Err(e);
                }
            }
        }
        if let Err(e) = complete_multipart_upload(state, &object_ref, upload_id, &parts).await {
            abort_upload_if_needed(state, &object_ref, Some(upload_id)).await;
            return Err(e);
        }
        state.metrics.object_offload_total.fetch_add(1, Ordering::Relaxed);
        return Ok(BodyLocation {
            body_base64: String::new(),
            object_ref,
            size: total_size,
            storage: "object",
        });
    }

    Ok(BodyLocation {
        body_base64: base64::engine::general_purpose::STANDARD.encode(&pending),
        object_ref: String::new(),
        size: total_size,
        storage: "inline",
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

async fn abort_upload_if_needed(state: &AppState, object_ref: &str, upload_id: Option<&str>) {
    let Some(upload_id) = upload_id else {
        return;
    };
    state.metrics.object_multipart_abort_total.fetch_add(1, Ordering::Relaxed);
    if let Err(abort_err) = abort_multipart_upload(state, object_ref, upload_id).await {
        tracing::warn!(object_ref = %object_ref, upload_id = %upload_id, error = %abort_err.message, "multipart upload abort failed");
    }
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

