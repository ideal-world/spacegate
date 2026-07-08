use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct ObservabilityConfig {
    pub enabled: bool,
    pub service_name: String,
    pub otlp_endpoint: String,
    pub protocol: OtlpProtocol,
    pub traces: TraceConfig,
    pub metrics: MetricConfig,
    pub logs: LogConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: "spacegate".to_string(),
            otlp_endpoint: "http://localhost:4317".to_string(),
            protocol: OtlpProtocol::Grpc,
            traces: TraceConfig::default(),
            metrics: MetricConfig::default(),
            logs: LogConfig::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum OtlpProtocol {
    #[default]
    Grpc,
    Http,
}

impl std::fmt::Display for OtlpProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OtlpProtocol::Grpc => write!(f, "grpc"),
            OtlpProtocol::Http => write!(f, "http"),
        }
    }
}

impl std::str::FromStr for OtlpProtocol {
    type Err = crate::BoxError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "grpc" => Ok(OtlpProtocol::Grpc),
            "http" | "http/protobuf" => Ok(OtlpProtocol::Http),
            _ => Err(format!("invalid otlp protocol: {s}").into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct TraceConfig {
    pub enabled: bool,
    pub sample_ratio: f64,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sample_ratio: 1.0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct MetricConfig {
    pub enabled: bool,
    pub export_interval_ms: u64,
}

impl Default for MetricConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            export_interval_ms: 60_000,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct LogConfig {
    pub enabled: bool,
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: "info".to_string(),
        }
    }
}
