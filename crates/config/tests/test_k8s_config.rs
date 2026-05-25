#[cfg(feature = "k8s")]
#[test]
fn observability_annotations_roundtrip() {
    use k8s_gateway_api::{Gateway, GatewaySpec};
    use kube::api::ObjectMeta;
    use spacegate_config::service::k8s::convert::gateway_k8s_conv::SgParametersConv;
    use spacegate_model::{ObservabilityConfig, OtlpProtocol, SgParameters};

    let params = SgParameters {
        observability: ObservabilityConfig {
            enabled: true,
            service_name: "spacegate-k8s".to_string(),
            otlp_endpoint: "http://otel-collector:4317".to_string(),
            protocol: OtlpProtocol::Grpc,
            traces: spacegate_model::TraceConfig {
                enabled: true,
                sample_ratio: 0.25,
            },
            metrics: spacegate_model::MetricConfig {
                enabled: true,
                export_interval_ms: 15000,
            },
            logs: spacegate_model::LogConfig {
                enabled: true,
                level: "info".to_string(),
            },
            ..Default::default()
        },
        ..Default::default()
    };

    let annotations = params.into_kube_gateway();
    let gateway = Gateway {
        metadata: ObjectMeta {
            annotations: Some(annotations),
            ..Default::default()
        },
        spec: GatewaySpec {
            gateway_class_name: Default::default(),
            listeners: Default::default(),
            addresses: Default::default(),
        },
        status: Default::default(),
    };

    let parsed = SgParameters::from_kube_gateway(&gateway);

    assert!(parsed.observability.enabled);
    assert_eq!(parsed.observability.service_name, "spacegate-k8s");
    assert_eq!(parsed.observability.otlp_endpoint, "http://otel-collector:4317");
    assert_eq!(parsed.observability.protocol, OtlpProtocol::Grpc);
    assert!(parsed.observability.traces.enabled);
    assert_eq!(parsed.observability.traces.sample_ratio, 0.25);
    assert!(parsed.observability.metrics.enabled);
    assert_eq!(parsed.observability.metrics.export_interval_ms, 15000);
    assert!(parsed.observability.logs.enabled);
    assert_eq!(parsed.observability.logs.level, "info");
}
