#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_log_path_excludes_query_parameters() {
        let uri: Uri = "/v1/admin/tenant-rate-limits?tenant=secret-tenant".parse().expect("valid URI");

        assert_eq!(request_log_path(&uri), "/v1/admin/tenant-rate-limits");
    }

    #[test]
    fn extracts_upload_id_from_multipart_xml() {
        let xml = "<InitiateMultipartUploadResult><UploadId>a+b/c=</UploadId></InitiateMultipartUploadResult>";
        assert_eq!(extract_xml_tag(xml, "UploadId").as_deref(), Some("a+b/c="));
    }

    #[test]
    fn encodes_upload_id_for_query_string() {
        assert_eq!(encode_query_component("a+b/c="), "a%2Bb%2Fc%3D");
    }

    #[test]
    fn builds_complete_multipart_xml_with_escaped_etags() {
        let parts = vec![
            CompletedPart {
                part_number: 1,
                etag: "\"abc&1\"".to_string(),
            },
            CompletedPart {
                part_number: 2,
                etag: "\"def\"".to_string(),
            },
        ];
        let xml = complete_multipart_xml(&parts);
        assert!(xml.contains("<PartNumber>1</PartNumber><ETag>&quot;abc&amp;1&quot;</ETag>"));
        assert!(xml.contains("<PartNumber>2</PartNumber><ETag>&quot;def&quot;</ETag>"));
    }

    #[test]
    fn callback_retry_delay_uses_exponential_backoff_with_cap() {
        assert_eq!(callback_retry_delay_ms(1000, 60_000, 1), 1000);
        assert_eq!(callback_retry_delay_ms(1000, 60_000, 3), 4000);
        assert_eq!(callback_retry_delay_ms(1000, 5000, 8), 5000);
    }

    #[test]
    fn parses_xpending_summary_count() {
        let value = Value::Array(vec![Value::Integer(7), Value::String("0-1".into()), Value::String("0-2".into())]);
        assert_eq!(pending_count_from_value(&value), 7);
    }

    #[test]
    fn observes_histogram_buckets_as_non_overlapping_counts() {
        let metrics = Metrics::default();
        observe_enqueue_latency(&metrics, 80, "queue", "inline");
        observe_enqueue_latency(&metrics, 800, "wait", "inline");
        observe_body_size(&metrics, 8 * 1024);
        observe_body_size(&metrics, 256 * 1024);
        observe_worker_processing(&metrics, 2000, "gpt-4o-mini");

        assert_eq!(metrics.enqueue_latency_count.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.enqueue_latency_le_100_ms.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.enqueue_latency_le_1000_ms.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.body_size_count.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.body_size_le_10kb.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.body_size_le_5mb.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.worker_processing_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.worker_processing_le_5000_ms.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn parses_tenant_rate_limit_json_and_csv() {
        let json = parse_tenant_rate_limit(r#"{"rps":10,"burst":20,"cost":3}"#).unwrap();
        assert_eq!(json.rps, 10);
        assert_eq!(json.burst, 20);
        assert_eq!(json.cost, 3);

        let csv = parse_tenant_rate_limit("15,30,2").unwrap();
        assert_eq!(csv.rps, 15);
        assert_eq!(csv.burst, 30);
        assert_eq!(csv.cost, 2);
    }

    #[test]
    fn rejects_tenant_rate_limit_cost_above_burst() {
        let rule = TenantRateLimitRule {
            tenant: "tenant-a".to_string(),
            model: None,
            path: None,
            policy: None,
            rps: 10,
            burst: 2,
            cost: 3,
            ttl_secs: None,
        };

        let err = validate_tenant_rate_limit_rule(&rule).expect_err("cost above burst must be rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("cost must be less than or equal to burst"));
    }

    #[test]
    fn rejects_colon_in_admin_rate_limit_dimensions() {
        for (field, rule) in [
            (
                "tenant",
                TenantRateLimitRule {
                    tenant: "tenant:a".to_string(),
                    model: None,
                    path: None,
                    policy: None,
                    rps: 10,
                    burst: 20,
                    cost: 1,
                    ttl_secs: None,
                },
            ),
            (
                "model",
                TenantRateLimitRule {
                    tenant: "tenant-a".to_string(),
                    model: Some("gpt:4".to_string()),
                    path: None,
                    policy: None,
                    rps: 10,
                    burst: 20,
                    cost: 1,
                    ttl_secs: None,
                },
            ),
            (
                "path",
                TenantRateLimitRule {
                    tenant: "tenant-a".to_string(),
                    model: None,
                    path: Some("/v1/chat:completions".to_string()),
                    policy: None,
                    rps: 10,
                    burst: 20,
                    cost: 1,
                    ttl_secs: None,
                },
            ),
        ] {
            let err = validate_tenant_rate_limit_rule(&rule).expect_err("colon dimension must be rejected");
            assert_eq!(err.status, StatusCode::BAD_REQUEST);
            assert!(err.message.contains(field));
            assert!(err.message.contains("must not contain ':'"));
        }
    }

    #[test]
    fn filters_hop_by_hop_headers_when_returning_upstream_body() {
        assert!(!should_return_upstream_header("content-length"));
        assert!(!should_return_upstream_header("Connection"));
        assert!(!should_return_upstream_header("transfer-encoding"));
        assert!(should_return_upstream_header("content-type"));
        assert!(should_return_upstream_header("x-request-id"));
    }

    #[test]
    fn poll_result_to_response_survives_realistic_upstream_headers() {
        let result = StoredResult {
            job_id: "01TEST".to_string(),
            status: "completed".to_string(),
            http_status: 200,
            headers: HashMap::from([
                ("content-type".to_string(), "application/json".to_string()),
                ("content-length".to_string(), "999".to_string()),
                ("connection".to_string(), "keep-alive".to_string()),
                ("server".to_string(), "elb".to_string()),
            ]),
            body_base64: base64::engine::general_purpose::STANDARD.encode(br#"{"ok":true}"#),
            completed_at_ms: 0,
            error: None,
        };
        let resp = poll_result_to_response(result).expect("poll response");
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get("content-length").is_none());
        assert!(resp.headers().get("connection").is_none());
    }

    #[test]
    fn parses_queue_priority_values() {
        assert_eq!(parse_queue_priority("HIGH"), Some(QueuePriority::High));
        assert_eq!(parse_queue_priority("medium"), Some(QueuePriority::Normal));
        assert_eq!(parse_queue_priority("low"), Some(QueuePriority::Low));
        assert_eq!(parse_queue_priority("urgent"), None);
    }

    async fn test_app_state(object_store_endpoint: Option<String>, inline_threshold: usize) -> AppState {
        let mut args = Args::parse_from(["ai-gateway-service"]);
        args.object_store_endpoint = object_store_endpoint;
        args.inline_threshold = inline_threshold;
        args.max_body_bytes = 8 * 1024 * 1024;
        args.object_store_bucket = "ai-gateway-body".to_string();
        args.object_store_prefix = "bodies".to_string();
        args.object_multipart_part_size = 1024;
        let redis = build_redis_client("redis://127.0.0.1/").expect("redis client");
        let wait_subscriber = WaitSubscriberHub::new("redis://127.0.0.1/").await.expect("wait subscriber");
        AppState {
            redis: redis.clone(),
            worker_redis: redis,
            http: reqwest::Client::new(),
            cfg: Arc::new(args),
            body_permits: Arc::new(Semaphore::new(8)),
            metrics: Arc::new(Metrics::default()),
            wait_subscriber,
        }
    }

    async fn mock_s3_handler(method: Method, uri: Uri, body: axum::body::Bytes, stored: Arc<std::sync::Mutex<Vec<u8>>>) -> Response {
        let query = uri.query().unwrap_or("");
        if method == Method::POST && query == "uploads" {
            return (
                StatusCode::OK,
                [(http::header::CONTENT_TYPE, "application/xml")],
                r#"<InitiateMultipartUploadResult><UploadId>test-upload</UploadId></InitiateMultipartUploadResult>"#,
            )
                .into_response();
        }
        if method == Method::PUT && query.contains("partNumber=") {
            stored.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(&body);
            return (StatusCode::OK, [(http::header::ETAG, "\"part-etag\"")]).into_response();
        }
        if method == Method::POST && query.contains("uploadId=") {
            return StatusCode::OK.into_response();
        }
        if method == Method::GET {
            let bytes = stored.lock().unwrap_or_else(|e| e.into_inner()).clone();
            return (StatusCode::OK, bytes).into_response();
        }
        StatusCode::NOT_FOUND.into_response()
    }

    #[tokio::test]
    async fn store_body_keeps_small_payload_inline() {
        let state = test_app_state(None, 16 * 1024).await;
        let payload = vec![1u8; 4096];
        let outcome = store_body(&state, "job-inline", Body::from(payload.clone())).await.expect("inline store");
        let location = outcome.location;
        assert_eq!(location.storage, "inline");
        assert_eq!(location.size, payload.len());
        assert!(!location.body_base64.is_empty());
        assert!(outcome.pending_upload.is_none());
        assert_eq!(state.metrics.object_offload_total.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn store_body_offloads_large_payload_via_s3_multipart_and_load_body_roundtrips() {
        let stored = Arc::new(std::sync::Mutex::new(Vec::new()));
        let stored_for_handler = stored.clone();
        let app = Router::new().fallback(move |method: Method, uri: Uri, body: axum::body::Bytes| {
            let stored_for_handler = stored_for_handler.clone();
            async move { mock_s3_handler(method, uri, body, stored_for_handler).await }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind mock s3");
        let addr = listener.local_addr().expect("mock s3 addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("mock s3 serve");
        });

        let state = test_app_state(Some(format!("http://{addr}")), 1024).await;
        let payload = vec![7u8; 5000];
        let outcome = store_body(&state, "job-offload", Body::from(payload.clone())).await.expect("offload store");
        if let Some(upload) = outcome.pending_upload {
            upload.await.expect("upload join").expect("upload body");
        }
        let location = outcome.location;
        assert_eq!(location.storage, "object");
        assert_eq!(location.size, payload.len());
        assert!(location.body_base64.is_empty());
        assert!(location.object_ref.contains("job-offload"));
        assert_eq!(state.metrics.object_offload_total.load(Ordering::Relaxed), 1);

        let mut fields = HashMap::new();
        fields.insert("storage".to_string(), Value::String("object".into()));
        fields.insert("ref".to_string(), Value::String(location.object_ref.into()));
        let loaded = load_body(&state, &fields).await.expect("load offloaded body");
        assert_eq!(loaded, payload);
    }
}
