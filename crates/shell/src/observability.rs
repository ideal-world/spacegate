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
    let _ = OTEL_GUARD.get_or_init(|| match build_guard(config) {
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
