use std::time::Duration;

use proxy_wasm::hostcalls;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use serde_json::Value;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Info);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(AiGatewayRoot::default()) });
}}

#[derive(Clone)]
struct AiGatewayConfig {
    service_cluster: String,
    service_authority: String,
    rate_limit_path: String,
    enqueue_path: String,
    wait_path: String,
    service_timeout_ms: u64,
    require_policy: bool,
    policy_header: String,
    tenant_header: String,
    model_header: String,
    priority_header: String,
    default_policy: Option<String>,
    priority_enabled: bool,
    default_priority: String,
    high_priority_models: Vec<String>,
    low_priority_models: Vec<String>,
    high_priority_tenants: Vec<String>,
    low_priority_tenants: Vec<String>,
}

impl Default for AiGatewayConfig {
    fn default() -> Self {
        Self {
            service_cluster: "ai-gateway-service".to_string(),
            service_authority: "ai-gateway-service".to_string(),
            rate_limit_path: "/v1/ratelimit/check".to_string(),
            enqueue_path: "/v1/queue/enqueue".to_string(),
            wait_path: "/v1/queue/enqueue-and-wait".to_string(),
            service_timeout_ms: 65_000,
            require_policy: true,
            policy_header: "x-ratelimit-policy".to_string(),
            tenant_header: "x-tenant-id".to_string(),
            model_header: "x-model".to_string(),
            priority_header: "x-queue-priority".to_string(),
            default_policy: None,
            priority_enabled: true,
            default_priority: "normal".to_string(),
            high_priority_models: Vec::new(),
            low_priority_models: Vec::new(),
            high_priority_tenants: Vec::new(),
            low_priority_tenants: Vec::new(),
        }
    }
}

impl AiGatewayConfig {
    fn parse(raw: &[u8]) -> Self {
        let mut cfg = Self::default();
        if raw.is_empty() {
            return cfg;
        }

        if let Ok(value) = serde_json::from_slice::<Value>(raw) {
            cfg.apply_json(&value);
            return cfg.normalized();
        }

        let text = String::from_utf8_lossy(raw);
        cfg.apply_legacy_lines(&text);
        cfg.normalized()
    }

    fn apply_json(&mut self, value: &Value) {
        set_string(value, &["service_cluster"], &mut self.service_cluster);
        set_string(value, &["service", "cluster"], &mut self.service_cluster);
        set_string(value, &["service_authority"], &mut self.service_authority);
        set_string(value, &["service", "authority"], &mut self.service_authority);
        set_string(value, &["rate_limit_path"], &mut self.rate_limit_path);
        set_string(value, &["paths", "rate_limit"], &mut self.rate_limit_path);
        set_string(value, &["enqueue_path"], &mut self.enqueue_path);
        set_string(value, &["paths", "enqueue"], &mut self.enqueue_path);
        set_string(value, &["wait_path"], &mut self.wait_path);
        set_string(value, &["paths", "wait"], &mut self.wait_path);
        set_u64(value, &["service_timeout_ms"], &mut self.service_timeout_ms);
        set_u64(value, &["service", "timeout_ms"], &mut self.service_timeout_ms);
        set_bool(value, &["require_policy"], &mut self.require_policy);
        set_bool(value, &["policies", "require"], &mut self.require_policy);
        self.default_policy = string_at(value, &["default_policy"]).or_else(|| string_at(value, &["policies", "default"]));
        set_string(value, &["policy_header"], &mut self.policy_header);
        set_string(value, &["headers", "policy"], &mut self.policy_header);
        set_string(value, &["tenant_header"], &mut self.tenant_header);
        set_string(value, &["headers", "tenant"], &mut self.tenant_header);
        set_string(value, &["model_header"], &mut self.model_header);
        set_string(value, &["headers", "model"], &mut self.model_header);
        set_string(value, &["priority_header"], &mut self.priority_header);
        set_string(value, &["headers", "priority"], &mut self.priority_header);
        set_bool(value, &["priority_enabled"], &mut self.priority_enabled);
        set_bool(value, &["priority", "enabled"], &mut self.priority_enabled);
        set_string(value, &["default_priority"], &mut self.default_priority);
        set_string(value, &["priority", "default"], &mut self.default_priority);
        self.high_priority_models = string_vec_at(value, &["priority", "high_models"]).or_else(|| string_vec_at(value, &["high_priority_models"])).unwrap_or_default();
        self.low_priority_models = string_vec_at(value, &["priority", "low_models"]).or_else(|| string_vec_at(value, &["low_priority_models"])).unwrap_or_default();
        self.high_priority_tenants = string_vec_at(value, &["priority", "high_tenants"]).or_else(|| string_vec_at(value, &["high_priority_tenants"])).unwrap_or_default();
        self.low_priority_tenants = string_vec_at(value, &["priority", "low_tenants"]).or_else(|| string_vec_at(value, &["low_priority_tenants"])).unwrap_or_default();
    }

    fn apply_legacy_lines(&mut self, text: &str) {
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else { continue };
            let key = key.trim().trim_matches(['"', '\'', '{', ',', ' '].as_ref());
            let value = value.trim().trim_matches(['"', '\'', ',', ' '].as_ref());
            match key {
                "service_cluster" => self.service_cluster = value.to_string(),
                "service_authority" => self.service_authority = value.to_string(),
                "rate_limit_path" => self.rate_limit_path = value.to_string(),
                "enqueue_path" => self.enqueue_path = value.to_string(),
                "wait_path" => self.wait_path = value.to_string(),
                "service_timeout_ms" => self.service_timeout_ms = value.parse().unwrap_or(self.service_timeout_ms),
                "require_policy" => self.require_policy = value.parse().unwrap_or(self.require_policy),
                "policy_header" => self.policy_header = value.to_string(),
                "tenant_header" => self.tenant_header = value.to_string(),
                "model_header" => self.model_header = value.to_string(),
                "priority_header" => self.priority_header = value.to_string(),
                "default_policy" => self.default_policy = Some(value.to_string()),
                "priority_enabled" => self.priority_enabled = value.parse().unwrap_or(self.priority_enabled),
                "default_priority" => self.default_priority = value.to_string(),
                "high_priority_models" => self.high_priority_models = parse_csv(value),
                "low_priority_models" => self.low_priority_models = parse_csv(value),
                "high_priority_tenants" => self.high_priority_tenants = parse_csv(value),
                "low_priority_tenants" => self.low_priority_tenants = parse_csv(value),
                _ => {}
            }
        }
    }

    fn normalized(mut self) -> Self {
        self.policy_header = normalize_header_name(&self.policy_header, "x-ratelimit-policy");
        self.tenant_header = normalize_header_name(&self.tenant_header, "x-tenant-id");
        self.model_header = normalize_header_name(&self.model_header, "x-model");
        self.priority_header = normalize_header_name(&self.priority_header, "x-queue-priority");
        self.default_priority = normalize_priority(&self.default_priority).unwrap_or_else(|| "normal".to_string());
        self.default_policy = self.default_policy.and_then(|value| normalize_policy(&value));
        self
    }
}

#[derive(Default)]
struct AiGatewayRoot {
    cfg: AiGatewayConfig,
}

impl Context for AiGatewayRoot {}

impl RootContext for AiGatewayRoot {
    fn on_vm_start(&mut self, _: usize) -> bool {
        let _ = hostcalls::log(LogLevel::Info, "ai-gateway-queue wasm plugin started");
        true
    }

    fn on_configure(&mut self, _: usize) -> bool {
        let raw = self.get_plugin_configuration().unwrap_or_default();
        self.cfg = AiGatewayConfig::parse(&raw);
        true
    }

    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(AiGatewayHttp {
            cfg: self.cfg.clone(),
            pending: None,
            deferred_policy: None,
        }))
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    Abandon,
    Queue,
    Wait,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    RateLimit,
    Queue,
    Wait,
}

struct AiGatewayHttp {
    cfg: AiGatewayConfig,
    pending: Option<(u32, Pending)>,
    deferred_policy: Option<Policy>,
}

impl Context for AiGatewayHttp {
    fn on_http_call_response(&mut self, token_id: u32, _num_headers: usize, body_size: usize, _num_trailers: usize) {
        let Some((pending_token, pending)) = self.pending else {
            return;
        };
        if token_id != pending_token {
            return;
        }
        self.pending = None;

        match pending {
            Pending::RateLimit => self.handle_rate_limit_response(body_size),
            Pending::Queue | Pending::Wait => self.forward_service_response(body_size),
        }
    }
}

impl HttpContext for AiGatewayHttp {
    fn on_http_request_headers(&mut self, _: usize, end_of_stream: bool) -> Action {
        let Some(policy) = self.request_policy() else {
            if self.cfg.require_policy {
                self.send_json(400, r#"{"error":"missing_or_invalid_rate_limit_policy"}"#);
                return Action::Pause;
            }
            return Action::Continue;
        };

        if self.tenant_id().is_none() {
            self.send_json(400, r#"{"error":"missing_x_tenant_id"}"#);
            return Action::Pause;
        }

        match policy {
            Policy::Abandon => {
                if self.dispatch_service_call(Pending::RateLimit, &self.cfg.rate_limit_path.clone(), None) {
                    Action::Pause
                } else {
                    self.send_json(502, r#"{"error":"rate_limit_service_unavailable"}"#);
                    Action::Pause
                }
            }
            Policy::Queue | Policy::Wait => {
                if end_of_stream {
                    let pending = if policy == Policy::Queue { Pending::Queue } else { Pending::Wait };
                    let path = if policy == Policy::Queue {
                        self.cfg.enqueue_path.clone()
                    } else {
                        self.cfg.wait_path.clone()
                    };
                    if !self.dispatch_service_call(pending, &path, Some(&[])) {
                        self.send_json(502, r#"{"error":"queue_service_unavailable"}"#);
                    }
                } else {
                    self.deferred_policy = Some(policy);
                }
                Action::Pause
            }
        }
    }

    fn on_http_request_body(&mut self, body_size: usize, end_of_stream: bool) -> Action {
        let Some(policy) = self.deferred_policy else {
            return Action::Continue;
        };
        if !end_of_stream {
            return Action::Pause;
        }
        self.deferred_policy = None;
        let body = self.get_http_request_body(0, body_size).unwrap_or_default();
        let pending = if policy == Policy::Queue { Pending::Queue } else { Pending::Wait };
        let path = if policy == Policy::Queue {
            self.cfg.enqueue_path.clone()
        } else {
            self.cfg.wait_path.clone()
        };
        if !self.dispatch_service_call(pending, &path, Some(&body)) {
            self.send_json(502, r#"{"error":"queue_service_unavailable"}"#);
        }
        Action::Pause
    }
}

impl AiGatewayHttp {
    fn request_policy(&self) -> Option<Policy> {
        let value = self.get_http_request_header(&self.cfg.policy_header).or_else(|| self.cfg.default_policy.clone())?;
        match normalize_policy(&value).as_deref() {
            Some("abandon") => Some(Policy::Abandon),
            Some("queue") => Some(Policy::Queue),
            Some("wait") => Some(Policy::Wait),
            _ => None,
        }
    }

    fn dispatch_service_call(&mut self, pending: Pending, path: &str, body: Option<&[u8]>) -> bool {
        let headers = self.service_headers(path);
        let refs = headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<Vec<_>>();
        match self.dispatch_http_call(&self.cfg.service_cluster, refs, body, vec![], Duration::from_millis(self.cfg.service_timeout_ms)) {
            Ok(token) => {
                self.pending = Some((token, pending));
                true
            }
            Err(status) => {
                let _ = hostcalls::log(LogLevel::Warn, &format!("dispatch service call failed: {status:?}"));
                false
            }
        }
    }

    fn service_headers(&self, path: &str) -> Vec<(String, String)> {
        let policy = self.request_policy().map(policy_name).unwrap_or("abandon").to_string();
        let tenant_id = self.tenant_id().unwrap_or_default();
        let model = self.model().unwrap_or_else(|| "default".to_string());
        let priority = self.queue_priority(&tenant_id, &model);
        let mut out = vec![
            (":method".to_string(), "POST".to_string()),
            (":path".to_string(), path.to_string()),
            (":authority".to_string(), self.cfg.service_authority.clone()),
            (
                "x-original-method".to_string(),
                self.get_http_request_header(":method").unwrap_or_else(|| "POST".to_string()),
            ),
            ("x-original-path".to_string(), self.get_http_request_header(":path").unwrap_or_else(|| "/".to_string())),
            ("x-ratelimit-policy".to_string(), policy),
            ("x-tenant-id".to_string(), tenant_id),
            ("x-model".to_string(), model),
        ];
        if let Some(priority) = priority {
            out.push(("x-queue-priority".to_string(), priority));
        }

        for (name, value) in self.get_http_request_headers() {
            if should_forward_to_service(&name) {
                out.push((name, value));
            }
        }
        out
    }

    fn tenant_id(&self) -> Option<String> {
        self.get_http_request_header(&self.cfg.tenant_header).filter(|value| !value.trim().is_empty())
    }

    fn model(&self) -> Option<String> {
        self.get_http_request_header(&self.cfg.model_header).filter(|value| !value.trim().is_empty())
    }

    fn queue_priority(&self, tenant_id: &str, model: &str) -> Option<String> {
        if !self.cfg.priority_enabled {
            return None;
        }
        if let Some(priority) = self.get_http_request_header(&self.cfg.priority_header).and_then(|value| normalize_priority(&value)) {
            return Some(priority);
        }
        if contains_value(&self.cfg.high_priority_tenants, tenant_id) || contains_value(&self.cfg.high_priority_models, model) {
            return Some("high".to_string());
        }
        if contains_value(&self.cfg.low_priority_tenants, tenant_id) || contains_value(&self.cfg.low_priority_models, model) {
            return Some("low".to_string());
        }
        normalize_priority(&self.cfg.default_priority)
    }

    fn handle_rate_limit_response(&mut self, body_size: usize) {
        let status = self.service_status();
        let body = self.get_http_call_response_body(0, body_size).unwrap_or_default();
        let text = String::from_utf8_lossy(&body);
        if status == 200 && contains_allowed_true(&text) {
            self.resume_http_request();
            return;
        }
        if status == 200 {
            let retry_after_ms = extract_json_number(&text, "retry_after_ms").unwrap_or(1000);
            let retry_after_secs = ((retry_after_ms + 999) / 1000).max(1).to_string();
            let retry_after_ms = retry_after_ms.to_string();
            let headers = [
                ("content-type".to_string(), "application/json".to_string()),
                ("retry-after".to_string(), retry_after_secs),
                ("x-ratelimit-retry-after-ms".to_string(), retry_after_ms),
            ];
            let headers = headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<Vec<_>>();
            self.send_http_response(429, headers, Some(text.as_bytes()));
        } else {
            self.send_json(502, r#"{"error":"rate_limit_service_error"}"#);
        }
    }

    fn forward_service_response(&mut self, body_size: usize) {
        let status = self.service_status();
        let body = self.get_http_call_response_body(0, body_size).unwrap_or_default();
        let header_storage = self.response_headers_for_client();
        let headers = header_storage.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<Vec<_>>();
        self.send_http_response(status as u32, headers, Some(&body));
    }

    fn response_headers_for_client(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for name in ["content-type", "x-job-id", "x-queue-wait-ms", "x-gateway-job-id", "retry-after", "location"] {
            if let Some(value) = self.get_http_call_response_header(name) {
                out.push((name.to_string(), value));
            }
        }
        out
    }

    fn service_status(&self) -> u16 {
        self.get_http_call_response_header(":status").and_then(|v| v.parse().ok()).unwrap_or(502)
    }

    fn send_json(&self, status: u32, body: &str) {
        self.send_http_response(status, vec![("content-type", "application/json")], Some(body.as_bytes()));
    }
}

fn should_forward_to_service(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !lower.starts_with(':')
        && !matches!(
            lower.as_str(),
            "host"
                | "connection"
                | "content-length"
                | "transfer-encoding"
                | "x-original-method"
                | "x-original-path"
                | "x-ratelimit-policy"
                | "x-tenant-id"
                | "x-model"
                | "x-queue-priority"
        )
}

fn policy_name(policy: Policy) -> &'static str {
    match policy {
        Policy::Abandon => "abandon",
        Policy::Queue => "queue",
        Policy::Wait => "wait",
    }
}

fn contains_allowed_true(text: &str) -> bool {
    text.contains(r#""allowed":true"#) || text.contains(r#""allowed": true"#)
}

fn extract_json_number(text: &str, key: &str) -> Option<u64> {
    let needle = format!(r#""{key}":"#);
    let pos = text.find(&needle)?;
    let digits = text[pos + needle.len()..].chars().skip_while(|c| c.is_whitespace()).take_while(|c| c.is_ascii_digit()).collect::<String>();
    digits.parse().ok()
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    value_at(value, path).and_then(|value| value.as_str().map(ToOwned::to_owned))
}

fn set_string(value: &Value, path: &[&str], target: &mut String) {
    if let Some(value) = string_at(value, path).filter(|value| !value.trim().is_empty()) {
        *target = value;
    }
}

fn set_u64(value: &Value, path: &[&str], target: &mut u64) {
    if let Some(value) = value_at(value, path).and_then(|value| value.as_u64()) {
        *target = value;
    }
}

fn set_bool(value: &Value, path: &[&str], target: &mut bool) {
    if let Some(value) = value_at(value, path).and_then(|value| value.as_bool()) {
        *target = value;
    }
}

fn string_vec_at(value: &Value, path: &[&str]) -> Option<Vec<String>> {
    let value = value_at(value, path)?;
    if let Some(raw) = value.as_str() {
        return Some(parse_csv(raw));
    }
    let values = value.as_array()?;
    Some(values.iter().filter_map(|value| value.as_str().map(ToOwned::to_owned)).collect())
}

fn parse_csv(value: &str) -> Vec<String> {
    value.split(',').map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned).collect()
}

fn normalize_header_name(value: &str, fallback: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() || value.starts_with(':') {
        fallback.to_string()
    } else {
        value
    }
}

fn normalize_policy(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "abandon" => Some("abandon".to_string()),
        "queue" => Some("queue".to_string()),
        "wait" => Some("wait".to_string()),
        _ => None,
    }
}

fn normalize_priority(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => Some("high".to_string()),
        "normal" | "default" | "medium" => Some("normal".to_string()),
        "low" => Some("low".to_string()),
        _ => None,
    }
}

fn contains_value(values: &[String], needle: &str) -> bool {
    values.iter().any(|value| value.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_json_config() {
        let cfg = AiGatewayConfig::parse(
            br#"{
                "service": {"cluster": "svc", "authority": "svc.local", "timeout_ms": 1200},
                "paths": {"rate_limit": "/rl", "enqueue": "/q", "wait": "/w"},
                "headers": {"policy": "X-Policy", "tenant": "X-Org", "model": "X-LLM", "priority": "X-Priority"},
                "policies": {"require": false, "default": "queue"},
                "priority": {"default": "low", "high_models": ["gpt-4"], "low_tenants": "free,basic"}
            }"#,
        );

        assert_eq!(cfg.service_cluster, "svc");
        assert_eq!(cfg.service_timeout_ms, 1200);
        assert_eq!(cfg.rate_limit_path, "/rl");
        assert_eq!(cfg.policy_header, "x-policy");
        assert!(!cfg.require_policy);
        assert_eq!(cfg.default_policy.as_deref(), Some("queue"));
        assert_eq!(cfg.default_priority, "low");
        assert_eq!(cfg.high_priority_models, vec!["gpt-4"]);
        assert_eq!(cfg.low_priority_tenants, vec!["free", "basic"]);
    }

    #[test]
    fn parses_legacy_config_lines() {
        let cfg = AiGatewayConfig::parse(
            br#"
service_cluster: ai-gateway
service_timeout_ms: 3000
tenant_header: X-Org
default_policy: wait
high_priority_models: qwen-max, deepseek-chat
"#,
        );

        assert_eq!(cfg.service_cluster, "ai-gateway");
        assert_eq!(cfg.service_timeout_ms, 3000);
        assert_eq!(cfg.tenant_header, "x-org");
        assert_eq!(cfg.default_policy.as_deref(), Some("wait"));
        assert_eq!(cfg.high_priority_models, vec!["qwen-max", "deepseek-chat"]);
    }
}
