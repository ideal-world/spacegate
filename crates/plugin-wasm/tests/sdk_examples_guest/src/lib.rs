//! 把 `proxy-wasm/proxy-wasm-rust-sdk` 仓库 `examples/` 下 4 个范例汇总进同一个 guest，
//! 由 `on_configure` 读到的 plugin configuration 选择运行模式：
//!
//! - `mode: headers`     ←→ examples/http_headers          （读 req/resp 头 + `/hello` 本地响应）
//! - `mode: body`        ←→ examples/http_body             （on_request_body 反转字节后落到 inner.call）
//! - `mode: config`      ←→ examples/http_config           （要求请求带某个 header，缺失则 403）
//! - `mode: auth_random` ←→ examples/http_auth_random      （`proxy_http_call` 到 "auth" cluster 决定放行）
//!
//! 之所以做成单 wasm + 模式切换，是为了集成测试只需构建一次 wasm 即可覆盖所有 SDK 范例。
//!
//! configuration 直接吃明文 YAML：第一行 `mode: <mode>`，第二行（可选）模式相关参数。
//! 这样不引入 serde_yaml/serde_json 依赖，wasm 体积更小、装配速度更快。

use log::{info, warn};
use proxy_wasm::traits::*;
use proxy_wasm::types::*;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(SdkRoot::default()) });
}}

#[derive(Default, Clone)]
struct SdkConfig {
    mode: Mode,
    /// `config` 模式：缺失即 403 的请求头名字（默认 `x-token`）。
    required_header: String,
    /// `auth_random` 模式：放行阈值。host 给出的 random byte < threshold 则视为允许。
    auth_threshold: u8,
    /// `auth_random` 模式：用于 dispatch_http_call 的 cluster 名。
    auth_cluster: String,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Mode {
    #[default]
    Noop,
    Headers,
    Body,
    Config,
    AuthRandom,
}

impl Mode {
    fn parse(s: &str) -> Self {
        match s.trim() {
            "headers" => Mode::Headers,
            "body" => Mode::Body,
            "config" => Mode::Config,
            "auth_random" => Mode::AuthRandom,
            _ => Mode::Noop,
        }
    }
}

#[derive(Default)]
struct SdkRoot {
    cfg: SdkConfig,
}

impl Context for SdkRoot {}

impl RootContext for SdkRoot {
    fn on_vm_start(&mut self, _: usize) -> bool { true }

    fn on_configure(&mut self, _: usize) -> bool {
        let raw = self.get_plugin_configuration().unwrap_or_default();
        let text = String::from_utf8_lossy(&raw);
        let mut cfg = SdkConfig::default();
        cfg.required_header = "x-token".into();
        cfg.auth_threshold = 128;
        cfg.auth_cluster = "auth".into();
        for line in text.lines() {
            let Some((k, v)) = line.split_once(':') else { continue };
            // 去掉 YAML 单/双引号
            let v = v.trim().trim_matches(['"', '\''].as_ref());
            match k.trim() {
                "mode" => cfg.mode = Mode::parse(v),
                "required_header" => cfg.required_header = v.to_string(),
                "auth_threshold" => cfg.auth_threshold = v.parse().unwrap_or(128),
                "auth_cluster" => cfg.auth_cluster = v.to_string(),
                _ => {}
            }
        }
        info!("sdk_examples_guest configured: mode_set={}", cfg.mode != Mode::Noop);
        self.cfg = cfg;
        true
    }

    fn create_http_context(&self, _context_id: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(SdkHttp {
            cfg: self.cfg.clone(),
            pending_token: None,
        }))
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }
}

struct SdkHttp {
    cfg: SdkConfig,
    pending_token: Option<u32>,
}

impl Context for SdkHttp {
    fn on_http_call_response(&mut self, token_id: u32, _num_headers: usize, body_size: usize, _num_trailers: usize) {
        // 只 auth_random 模式会到这里；其它模式根本没发 dispatch。
        if Some(token_id) != self.pending_token { return; }
        self.pending_token = None;
        let body = self
            .get_http_call_response_body(0, body_size)
            .unwrap_or_default();
        let allow = body.first().map(|b| *b < self.cfg.auth_threshold).unwrap_or(false);
        if allow {
            // 放行：把 Pause 恢复 → 让 host 继续 inner.call。
            self.resume_http_request();
        } else {
            self.send_http_response(403, vec![("x-rejected-by", "auth_random")], Some(b"forbidden"));
        }
    }
}

impl HttpContext for SdkHttp {
    fn on_http_request_headers(&mut self, _: usize, _: bool) -> Action {
        match self.cfg.mode {
            Mode::Headers => {
                for (name, value) in &self.get_http_request_headers() {
                    info!("-> {name}: {value}");
                }
                match self.get_http_request_header(":path") {
                    Some(p) if p == "/hello" => {
                        self.send_http_response(
                            200,
                            vec![("hello", "world"), ("powered-by", "proxy-wasm")],
                            Some(b"Hello, World!\n"),
                        );
                        Action::Pause
                    }
                    _ => Action::Continue,
                }
            }
            Mode::Body => Action::Continue,
            Mode::Config => {
                if self.get_http_request_header(&self.cfg.required_header).is_some() {
                    Action::Continue
                } else {
                    self.send_http_response(
                        403,
                        vec![("x-rejected-by", "http_config")],
                        Some(b"missing required header"),
                    );
                    Action::Pause
                }
            }
            Mode::AuthRandom => {
                match self.dispatch_http_call(
                    &self.cfg.auth_cluster.clone(),
                    vec![(":method", "GET"), (":path", "/random"), (":authority", "auth")],
                    None,
                    vec![],
                    std::time::Duration::from_millis(500),
                ) {
                    Ok(token) => {
                        self.pending_token = Some(token);
                        Action::Pause
                    }
                    Err(s) => {
                        warn!("dispatch_http_call failed: status={s:?}");
                        self.send_http_response(502, vec![("x-rejected-by", "dispatch_failed")], Some(b"upstream auth unreachable"));
                        Action::Pause
                    }
                }
            }
            Mode::Noop => Action::Continue,
        }
    }

    fn on_http_request_body(&mut self, body_size: usize, end_of_stream: bool) -> Action {
        if !matches!(self.cfg.mode, Mode::Body) || !end_of_stream {
            return Action::Continue;
        }
        if body_size == 0 { return Action::Continue; }
        let body = self.get_http_request_body(0, body_size).unwrap_or_default();
        let mut rev = body.clone();
        rev.reverse();
        // spec §Buffers：start=0, size=原长度 → 替换；len(value)=新长度。
        let _ = self.set_http_request_body(0, body.len(), &rev);
        Action::Continue
    }

    fn on_http_response_headers(&mut self, _: usize, _: bool) -> Action {
        if matches!(self.cfg.mode, Mode::Headers) {
            for (name, value) in &self.get_http_response_headers() {
                info!("<- {name}: {value}");
            }
            // 给响应额外塞一条头：SDK 那个 example 没塞，我们这里塞便于测试断言。
            let _ = self.add_http_response_header("x-sdk-headers", "seen");
        }
        Action::Continue
    }

    fn on_log(&mut self) {
        info!("sdk_examples_guest: ctx done.");
    }
}
