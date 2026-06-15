use std::sync::OnceLock;
use std::time::Duration;

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    trace::{Sampler, SdkTracerProvider},
    Resource,
};
use spacegate_config::model::{ObservabilityConfig, OtlpProtocol};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

static OTEL_GUARD: OnceLock<ObservabilityGuard> = OnceLock::new();

#[derive(Debug)]
pub struct ObservabilityGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

impl ObservabilityGuard {
    pub fn shutdown(&self) {
        if let Some(provider) = &self.tracer_provider {
            if let Err(err) = provider.shutdown() {
                eprintln!("failed to shutdown otel tracer provider: {err}");
            }
        }
        if let Some(provider) = &self.meter_provider {
            if let Err(err) = provider.shutdown() {
                eprintln!("failed to shutdown otel meter provider: {err}");
            }
        }
        if let Some(provider) = &self.logger_provider {
            if let Err(err) = provider.shutdown() {
                eprintln!("failed to shutdown otel logger provider: {err}");
            }
        }
    }
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn init(config: &ObservabilityConfig) {
    let config = config_with_env_overrides(config, |key| std::env::var(key));
    let _ = OTEL_GUARD.get_or_init(|| match build_guard(&config) {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("failed to initialize OpenTelemetry, falling back to stdout tracing: {err}");
            init_stdout_only();
            ObservabilityGuard {
                tracer_provider: None,
                meter_provider: None,
                logger_provider: None,
            }
        }
    });
}

fn config_with_env_overrides<F>(config: &ObservabilityConfig, mut get_env: F) -> ObservabilityConfig
where
    F: FnMut(&str) -> Result<String, std::env::VarError>,
{
    let mut config = config.clone();
    if let Ok(value) = get_env("SPACEGATE_OTEL_ENABLED") {
        if let Ok(value) = value.parse::<bool>() {
            config.enabled = value;
        }
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_SERVICE_NAME") {
        config.service_name = value;
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_ENDPOINT") {
        config.otlp_endpoint = value;
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_PROTOCOL") {
        if let Ok(value) = value.parse::<OtlpProtocol>() {
            config.protocol = value;
        }
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_TRACES_ENABLED") {
        if let Ok(value) = value.parse::<bool>() {
            config.traces.enabled = value;
        }
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_TRACES_SAMPLE_RATIO") {
        if let Ok(value) = value.parse::<f64>() {
            config.traces.sample_ratio = value;
        }
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_METRICS_ENABLED") {
        if let Ok(value) = value.parse::<bool>() {
            config.metrics.enabled = value;
        }
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_METRICS_EXPORT_INTERVAL_MS") {
        if let Ok(value) = value.parse::<u64>() {
            config.metrics.export_interval_ms = value;
        }
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_LOGS_ENABLED") {
        if let Ok(value) = value.parse::<bool>() {
            config.logs.enabled = value;
        }
    }
    if let Ok(value) = get_env("SPACEGATE_OTEL_LOGS_LEVEL") {
        config.logs.level = value;
    }
    config
}

fn build_guard(config: &ObservabilityConfig) -> Result<ObservabilityGuard, Box<dyn std::error::Error + Send + Sync>> {
    let env_filter = EnvFilter::from_default_env();
    let fmt_layer = tracing_subscriber::fmt::layer();
    if !config.enabled {
        tracing_subscriber::registry().with(env_filter).with(fmt_layer).try_init()?;
        return Ok(ObservabilityGuard {
            tracer_provider: None,
            meter_provider: None,
            logger_provider: None,
        });
    }

    let resource = Resource::builder().with_service_name(config.service_name.clone()).build();
    let mut guard = ObservabilityGuard {
        tracer_provider: None,
        meter_provider: None,
        logger_provider: None,
    };

    let trace_layer = if config.traces.enabled {
        match build_span_exporter(config) {
            Ok(exporter) => {
                let provider = SdkTracerProvider::builder()
                    .with_resource(resource.clone())
                    .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(config.traces.sample_ratio))))
                    .with_batch_exporter(exporter)
                    .build();
                let tracer = provider.tracer("spacegate");
                global::set_tracer_provider(provider.clone());
                guard.tracer_provider = Some(provider);
                Some(tracing_opentelemetry::layer().with_tracer(tracer).boxed())
            }
            Err(err) => {
                eprintln!("failed to initialize OpenTelemetry traces, disabling traces: {err}");
                None
            }
        }
    } else {
        None
    };

    if config.metrics.enabled {
        match build_metric_exporter(config) {
            Ok(exporter) => {
                let reader = PeriodicReader::builder(exporter).with_interval(metric_export_interval(config)).build();
                let provider = SdkMeterProvider::builder().with_resource(resource.clone()).with_reader(reader).build();
                global::set_meter_provider(provider.clone());
                guard.meter_provider = Some(provider);
            }
            Err(err) => {
                eprintln!("failed to initialize OpenTelemetry metrics, disabling metrics: {err}");
            }
        }
    }

    let log_layer = if config.logs.enabled {
        match build_log_exporter(config) {
            Ok(exporter) => {
                let provider = SdkLoggerProvider::builder().with_resource(resource).with_batch_exporter(exporter).build();
                let level_filter = log_level_filter(config);
                let layer = OpenTelemetryTracingBridge::new(&provider).with_filter(level_filter).boxed();
                guard.logger_provider = Some(provider);
                Some(layer)
            }
            Err(err) => {
                eprintln!("failed to initialize OpenTelemetry logs, disabling logs: {err}");
                None
            }
        }
    } else {
        None
    };

    tracing_subscriber::registry().with(env_filter).with(fmt_layer).with(trace_layer).with(log_layer).try_init()?;
    Ok(guard)
}

fn init_stdout_only() {
    let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).try_init();
}

fn otlp_protocol(config: &ObservabilityConfig) -> opentelemetry_otlp::Protocol {
    match config.protocol {
        OtlpProtocol::Grpc => opentelemetry_otlp::Protocol::Grpc,
        OtlpProtocol::Http => opentelemetry_otlp::Protocol::HttpBinary,
    }
}

fn metric_export_interval(config: &ObservabilityConfig) -> Duration {
    Duration::from_millis(config.metrics.export_interval_ms)
}

fn log_level_filter(config: &ObservabilityConfig) -> tracing_subscriber::filter::LevelFilter {
    config.logs.level.parse::<tracing_subscriber::filter::LevelFilter>().unwrap_or(tracing_subscriber::filter::LevelFilter::INFO)
}

fn build_span_exporter(config: &ObservabilityConfig) -> Result<opentelemetry_otlp::SpanExporter, opentelemetry_otlp::ExporterBuildError> {
    let timeout = Duration::from_secs(5);
    match config.protocol {
        OtlpProtocol::Grpc => opentelemetry_otlp::SpanExporter::builder().with_tonic().with_endpoint(config.otlp_endpoint.clone()).with_timeout(timeout).build(),
        OtlpProtocol::Http => {
            opentelemetry_otlp::SpanExporter::builder().with_http().with_endpoint(config.otlp_endpoint.clone()).with_protocol(otlp_protocol(config)).with_timeout(timeout).build()
        }
    }
}

#[cfg(test)]
mod tests {
    use spacegate_config::model::{LogConfig, MetricConfig};

    use super::*;

    #[test]
    fn metric_export_interval_uses_configured_millis() {
        let config = ObservabilityConfig {
            metrics: MetricConfig {
                enabled: true,
                export_interval_ms: 15_000,
            },
            ..Default::default()
        };

        assert_eq!(metric_export_interval(&config), Duration::from_secs(15));
    }

    #[test]
    fn invalid_log_level_falls_back_to_info() {
        let config = ObservabilityConfig {
            logs: LogConfig {
                enabled: true,
                level: "not-a-level".to_string(),
            },
            ..Default::default()
        };

        assert_eq!(log_level_filter(&config), tracing_subscriber::filter::LevelFilter::INFO);
    }

    #[test]
    fn env_overrides_observability_config() {
        let config = config_with_env_overrides(&ObservabilityConfig::default(), |key| match key {
            "SPACEGATE_OTEL_ENABLED" => Ok("true".to_string()),
            "SPACEGATE_OTEL_SERVICE_NAME" => Ok("spacegate-k8s-test".to_string()),
            "SPACEGATE_OTEL_ENDPOINT" => Ok("http://otel-collector:4317".to_string()),
            "SPACEGATE_OTEL_PROTOCOL" => Ok("grpc".to_string()),
            "SPACEGATE_OTEL_TRACES_ENABLED" => Ok("true".to_string()),
            "SPACEGATE_OTEL_TRACES_SAMPLE_RATIO" => Ok("0.5".to_string()),
            "SPACEGATE_OTEL_METRICS_ENABLED" => Ok("true".to_string()),
            "SPACEGATE_OTEL_METRICS_EXPORT_INTERVAL_MS" => Ok("15000".to_string()),
            "SPACEGATE_OTEL_LOGS_ENABLED" => Ok("true".to_string()),
            "SPACEGATE_OTEL_LOGS_LEVEL" => Ok("warn".to_string()),
            _ => Err(std::env::VarError::NotPresent),
        });

        assert!(config.enabled);
        assert_eq!(config.service_name, "spacegate-k8s-test");
        assert_eq!(config.otlp_endpoint, "http://otel-collector:4317");
        assert_eq!(config.protocol, OtlpProtocol::Grpc);
        assert!(config.traces.enabled);
        assert_eq!(config.traces.sample_ratio, 0.5);
        assert!(config.metrics.enabled);
        assert_eq!(config.metrics.export_interval_ms, 15000);
        assert!(config.logs.enabled);
        assert_eq!(config.logs.level, "warn");
    }
}

fn build_metric_exporter(config: &ObservabilityConfig) -> Result<opentelemetry_otlp::MetricExporter, opentelemetry_otlp::ExporterBuildError> {
    let timeout = Duration::from_secs(5);
    match config.protocol {
        OtlpProtocol::Grpc => opentelemetry_otlp::MetricExporter::builder().with_tonic().with_endpoint(config.otlp_endpoint.clone()).with_timeout(timeout).build(),
        OtlpProtocol::Http => {
            opentelemetry_otlp::MetricExporter::builder().with_http().with_endpoint(config.otlp_endpoint.clone()).with_protocol(otlp_protocol(config)).with_timeout(timeout).build()
        }
    }
}

fn build_log_exporter(config: &ObservabilityConfig) -> Result<opentelemetry_otlp::LogExporter, opentelemetry_otlp::ExporterBuildError> {
    let timeout = Duration::from_secs(5);
    match config.protocol {
        OtlpProtocol::Grpc => opentelemetry_otlp::LogExporter::builder().with_tonic().with_endpoint(config.otlp_endpoint.clone()).with_timeout(timeout).build(),
        OtlpProtocol::Http => {
            opentelemetry_otlp::LogExporter::builder().with_http().with_endpoint(config.otlp_endpoint.clone()).with_protocol(otlp_protocol(config)).with_timeout(timeout).build()
        }
    }
}
