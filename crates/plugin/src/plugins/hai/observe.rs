use std::sync::OnceLock;
use std::time::Duration;

use hyper::{Request, Response};
use opentelemetry::{global, KeyValue};
use serde::{Deserialize, Serialize};
use spacegate_kernel::{extension::GatewayName, helper_layers::function::Inner, observability::TelemetryContext, BoxError, SgBody};

use crate::Plugin;

use super::{
    types::{HaiAuditData, HaiAuditState},
    usage, util,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HaiObserveConfig {
    #[serde(default = "default_true")]
    pub audit_log_enabled: bool,
    #[serde(default = "default_true")]
    pub llm_metrics_enabled: bool,
    #[serde(default = "default_true")]
    pub output_guard_enabled: bool,
    #[serde(default)]
    pub allowed_output_targets: Vec<String>,
    #[serde(default = "default_model_error_keywords")]
    pub model_error_keywords: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_model_error_keywords() -> Vec<String> {
    vec!["error".to_string(), "ERR_".to_string(), "FAILED".to_string()]
}

impl Default for HaiObserveConfig {
    fn default() -> Self {
        Self {
            audit_log_enabled: true,
            llm_metrics_enabled: true,
            output_guard_enabled: true,
            allowed_output_targets: Vec::new(),
            model_error_keywords: default_model_error_keywords(),
        }
    }
}

pub struct HaiObservePlugin {
    config: HaiObserveConfig,
}

impl HaiObservePlugin {
    fn write_telemetry(&self, telemetry: Option<&TelemetryContext>, data: &HaiAuditData) {
        let Some(telemetry) = telemetry else {
            return;
        };
        let request = data.request.as_ref();
        let asset = data.asset.as_ref();
        let dispatch = data.dispatch.as_ref();
        let usage = &data.usage.usage;

        let fields = [
            ("request_id", request.map(|v| v.request_id.clone())),
            ("app_id", data.identity.as_ref().map(|v| v.app_id.clone())),
            ("api_key_hash", request.map(|v| v.api_key_hash.clone())),
            ("client_ip", request.map(|v| v.client_ip.clone())),
            ("client_mac", request.map(|v| v.client_mac.clone())),
            ("asset_id", request.map(|v| v.asset_id.clone()).or_else(|| asset.map(|v| v.asset_id.clone()))),
            ("asset_type", asset.map(|v| v.asset_type.clone())),
            ("asset_version", request.and_then(|v| v.asset_version.clone())),
            ("protocol", dispatch.map(|v| v.protocol.clone())),
            ("upstream", dispatch.and_then(|v| v.upstream.as_ref().map(ToString::to_string))),
            ("timeout_ms", dispatch.and_then(|v| v.timeout_ms.map(|timeout| timeout.to_string()))),
            ("is_model", dispatch.map(|v| v.is_model.to_string())),
            ("is_streaming", dispatch.map(|v| v.is_streaming.to_string())),
            ("success", Some(data.usage.status.map(|s| (200..300).contains(&s)).unwrap_or(false).to_string())),
            ("error_code", data.usage.error_code.clone()),
            ("upstream_status", data.usage.status.map(|v| v.to_string())),
            ("duration_ms", Some(data.usage.started_at.elapsed().as_millis().to_string())),
            ("prompt_tokens", usage.prompt_tokens.map(|v| v.to_string())),
            ("completion_tokens", usage.completion_tokens.map(|v| v.to_string())),
            ("total_tokens", usage.total_tokens.map(|v| v.to_string())),
            ("cache_hit_tokens", usage.cache_hit_tokens.map(|v| v.to_string())),
            ("cache_miss_tokens", usage.cache_miss_tokens.map(|v| v.to_string())),
            ("output_blocked", Some(data.usage.output_blocked.to_string())),
            ("output_block_reason", data.usage.output_block_reason.clone()),
        ];
        for (key, value) in fields {
            if let Some(value) = value {
                let _ = telemetry.insert_namespaced("hai", key, value);
            }
        }
    }

    fn record_metrics(&self, gateway: String, data: &HaiAuditData, duration: Duration) {
        if !self.config.llm_metrics_enabled {
            return;
        }
        let asset = data.asset.as_ref();
        let dispatch = data.dispatch.as_ref();
        let success = data.usage.status.map(|s| (200..300).contains(&s)).unwrap_or(false).to_string();
        let attrs = [
            KeyValue::new("gateway", gateway),
            KeyValue::new("asset_id", asset.map(|v| v.asset_id.clone()).unwrap_or_default()),
            KeyValue::new("asset_type", asset.map(|v| v.asset_type.clone()).unwrap_or_default()),
            KeyValue::new("protocol", dispatch.map(|v| v.protocol.clone()).unwrap_or_default()),
            KeyValue::new("success", success),
            KeyValue::new("error_code", data.usage.error_code.clone().unwrap_or_default()),
        ];
        let instruments = llm_instruments();
        instruments.invocations.add(1, &attrs);
        instruments.duration.record(duration.as_secs_f64(), &attrs);
        if data.usage.status.is_some_and(|status| !(200..300).contains(&status)) {
            instruments.errors.add(1, &attrs);
        }
        if data.usage.output_blocked {
            instruments.output_blocked.add(1, &attrs);
        }
        add_optional(&instruments.prompt_tokens, data.usage.usage.prompt_tokens, &attrs);
        add_optional(&instruments.completion_tokens, data.usage.usage.completion_tokens, &attrs);
        add_optional(&instruments.total_tokens, data.usage.usage.total_tokens, &attrs);
    }

    fn audit_log(&self, gateway: String, data: &HaiAuditData, duration: Duration) {
        if !self.config.audit_log_enabled {
            return;
        }
        let request = data.request.as_ref();
        let asset = data.asset.as_ref();
        let dispatch = data.dispatch.as_ref();
        let usage = &data.usage.usage;
        let success = data.usage.status.map(|s| (200..300).contains(&s)).unwrap_or(false);
        tracing::info!(
            event = "hai_llm_invocation",
            gateway = %gateway,
            request_id = %request.map(|v| v.request_id.as_str()).unwrap_or_default(),
            app_id = %data.identity.as_ref().map(|v| v.app_id.as_str()).unwrap_or_default(),
            api_key_hash = %request.map(|v| v.api_key_hash.as_str()).unwrap_or_default(),
            client_ip = %request.map(|v| v.client_ip.as_str()).unwrap_or_default(),
            asset_id = %request.map(|v| v.asset_id.as_str()).or_else(|| asset.map(|v| v.asset_id.as_str())).unwrap_or_default(),
            asset_type = %asset.map(|v| v.asset_type.as_str()).unwrap_or_default(),
            asset_version = %request.and_then(|v| v.asset_version.as_deref()).unwrap_or_default(),
            protocol = %dispatch.map(|v| v.protocol.as_str()).unwrap_or_default(),
            upstream = %dispatch.and_then(|v| v.upstream.as_ref()).map(ToString::to_string).unwrap_or_default(),
            timeout_ms = ?dispatch.and_then(|v| v.timeout_ms),
            is_model = dispatch.map(|v| v.is_model).unwrap_or(false),
            is_streaming = dispatch.map(|v| v.is_streaming).unwrap_or(false),
            success = success,
            error_code = %data.usage.error_code.as_deref().unwrap_or_default(),
            upstream_status = ?data.usage.status,
            duration_ms = duration.as_millis() as u64,
            prompt_tokens = ?usage.prompt_tokens,
            completion_tokens = ?usage.completion_tokens,
            total_tokens = ?usage.total_tokens,
            cache_hit_tokens = ?usage.cache_hit_tokens,
            cache_miss_tokens = ?usage.cache_miss_tokens,
            output_blocked = data.usage.output_blocked,
            output_block_reason = %data.usage.output_block_reason.as_deref().unwrap_or_default(),
            "hai llm invocation audit"
        );
    }
}

impl Plugin for HaiObservePlugin {
    const CODE: &'static str = "hai-observe";

    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let config = serde_json::from_value::<HaiObserveConfig>(config.spec)?;
        Ok(Self { config })
    }

    async fn call(&self, mut req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let audit = HaiAuditState::default();
        let telemetry = req.extensions().get::<TelemetryContext>().cloned();
        let gateway = req.extensions().get::<GatewayName>().map(|v| v.to_string()).unwrap_or_default();
        req.extensions_mut().insert(audit.clone());

        let resp = inner.call(req).await;
        let status = resp.status().as_u16();
        let is_sse = util::content_type_is_sse(resp.headers());
        let (parts, body) = resp.into_parts();
        let body = body.dump().await?;
        let dumped = body.get_dumped().cloned().unwrap_or_default();
        let mut usage = if is_sse {
            usage::parse_usage_from_sse_chunk(&dumped)
        } else {
            usage::parse_usage_from_json_bytes(&dumped)
        }
        .unwrap_or_default();
        if !is_sse {
            if let Some(sse_usage) = usage::parse_usage_from_sse_chunk(&dumped) {
                usage.merge_from(sse_usage);
            }
        }

        audit.update(|data| {
            data.usage.status = Some(status);
            data.usage.usage.merge_from(usage);
            if !(200..300).contains(&status) {
                data.usage.error_code = Some(if status == 504 { "upstream_timeout" } else { "upstream_error" }.to_string());
            }
        });

        let data = audit.snapshot();
        let duration = data.usage.started_at.elapsed();
        self.write_telemetry(telemetry.as_ref(), &data);
        self.record_metrics(gateway.clone(), &data, duration);
        self.audit_log(gateway, &data, duration);

        Ok(Response::from_parts(parts, body))
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

fn add_optional(counter: &opentelemetry::metrics::Counter<u64>, value: Option<u64>, attrs: &[KeyValue]) {
    if let Some(value) = value {
        counter.add(value, attrs);
    }
}

#[derive(Debug)]
struct LlmInstruments {
    invocations: opentelemetry::metrics::Counter<u64>,
    errors: opentelemetry::metrics::Counter<u64>,
    output_blocked: opentelemetry::metrics::Counter<u64>,
    prompt_tokens: opentelemetry::metrics::Counter<u64>,
    completion_tokens: opentelemetry::metrics::Counter<u64>,
    total_tokens: opentelemetry::metrics::Counter<u64>,
    duration: opentelemetry::metrics::Histogram<f64>,
}

fn llm_instruments() -> &'static LlmInstruments {
    static INSTRUMENTS: OnceLock<LlmInstruments> = OnceLock::new();
    INSTRUMENTS.get_or_init(|| {
        let meter = global::meter("spacegate_plugin_hai");
        LlmInstruments {
            invocations: meter.u64_counter("hai.llm.invocations").build(),
            errors: meter.u64_counter("hai.llm.errors").build(),
            output_blocked: meter.u64_counter("hai.llm.output_blocked").build(),
            prompt_tokens: meter.u64_counter("hai.llm.prompt_tokens").build(),
            completion_tokens: meter.u64_counter("hai.llm.completion_tokens").build(),
            total_tokens: meter.u64_counter("hai.llm.total_tokens").build(),
            duration: meter.f64_histogram("hai.llm.duration").with_unit("s").build(),
        }
    })
}

#[cfg(feature = "schema")]
crate::schema!(HaiObservePlugin, HaiObserveConfig);
