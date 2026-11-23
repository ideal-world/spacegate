use serde::{Deserialize, Serialize};

/// 进程首次初始化后冻结的 OTLP 生效快照。只读管理接口不返回认证头或 endpoint 凭据。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObservabilityRuntimeSnapshot {
    pub config: ObservabilityConfig,
    /// 表示本地导出器已建立，不代表 Collector 可达或数据已写入。
    pub traces_active: bool,
    pub metrics_active: bool,
    pub logs_active: bool,
    pub initialization_failed: bool,
}

/// 由运行中的进程报告其实际承载的网关名称，防止管理端展示其他进程的配置。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GatewayObservabilityRuntime {
    pub gateways: Vec<String>,
    pub observability: Option<ObservabilityRuntimeSnapshot>,
}

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
