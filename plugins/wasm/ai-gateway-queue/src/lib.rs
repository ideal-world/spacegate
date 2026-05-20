use std::time::Duration;

use proxy_wasm::hostcalls;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;

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
        }
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
        let text = String::from_utf8_lossy(&raw);
        let mut cfg = AiGatewayConfig::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else { continue };
            let key = key.trim().trim_matches(['"', '\'', '{', ',', ' '].as_ref());
            let value = value.trim().trim_matches(['"', '\'', ',', ' '].as_ref());
            match key {
                "service_cluster" => cfg.service_cluster = value.to_string(),
                "service_authority" => cfg.service_authority = value.to_string(),
                "rate_limit_path" => cfg.rate_limit_path = value.to_string(),
                "enqueue_path" => cfg.enqueue_path = value.to_string(),
                "wait_path" => cfg.wait_path = value.to_string(),
                "service_timeout_ms" => cfg.service_timeout_ms = value.parse().unwrap_or(cfg.service_timeout_ms),
                "require_policy" => cfg.require_policy = value.parse().unwrap_or(true),
                _ => {}
            }
        }
        self.cfg = cfg;
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

        if self.get_http_request_header("x-tenant-id").is_none() {
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
        match self.get_http_request_header("x-ratelimit-policy").as_deref().map(str::trim) {
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
        let mut out = vec![
            (":method".to_string(), "POST".to_string()),
            (":path".to_string(), path.to_string()),
            (":authority".to_string(), self.cfg.service_authority.clone()),
            (
                "x-original-method".to_string(),
                self.get_http_request_header(":method").unwrap_or_else(|| "POST".to_string()),
            ),
            ("x-original-path".to_string(), self.get_http_request_header(":path").unwrap_or_else(|| "/".to_string())),
        ];

        for (name, value) in self.get_http_request_headers() {
            if should_forward_to_service(&name) {
                out.push((name, value));
            }
        }
        out
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
            "host" | "connection" | "content-length" | "transfer-encoding" | "x-original-method" | "x-original-path"
        )
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
