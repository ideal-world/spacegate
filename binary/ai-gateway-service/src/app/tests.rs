#[cfg(test)]
mod tests {
    use super::*;

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
        observe_enqueue_latency(&metrics, 80);
        observe_enqueue_latency(&metrics, 800);
        observe_body_size(&metrics, 8 * 1024);
        observe_body_size(&metrics, 256 * 1024);
        observe_worker_processing(&metrics, 2000);

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
    fn parses_queue_priority_values() {
        assert_eq!(parse_queue_priority("HIGH"), Some(QueuePriority::High));
        assert_eq!(parse_queue_priority("medium"), Some(QueuePriority::Normal));
        assert_eq!(parse_queue_priority("low"), Some(QueuePriority::Low));
        assert_eq!(parse_queue_priority("urgent"), None);
    }
}
