//! 记录 Proxy-Wasm response-body 回调，并逐块改写 SSE `data:` payload。

use proxy_wasm::hostcalls;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;

proxy_wasm::main! {{
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(StreamingRoot::default()) });
}}

/// 从插件配置中读取测试隔离使用的 shared-data key。
#[derive(Default)]
struct StreamingRoot {
    state_key: String,
    mode: StreamingMode,
}

/// 测试 guest 的响应处理模式。
#[derive(Clone, Copy, Default)]
enum StreamingMode {
    #[default]
    Rewrite,
    Usage,
    Block,
}

impl Context for StreamingRoot {}

impl RootContext for StreamingRoot {
    /// 解析 `state_key`，使并行测试不会共享同一个回调计数器。
    fn on_configure(&mut self, _: usize) -> bool {
        let raw = self.get_plugin_configuration().unwrap_or_default();
        let text = String::from_utf8_lossy(&raw);
        self.state_key = text
            .lines()
            .find_map(|line| line.split_once(':').filter(|(key, _)| key.trim() == "state_key").map(|(_, value)| value.trim().trim_matches(['\"', '\'']).to_string()))
            .unwrap_or_else(|| "streaming-body.default".to_string());
        self.mode = text
            .lines()
            .find_map(|line| line.split_once(':').filter(|(key, _)| key.trim() == "mode").map(|(_, value)| value.trim().trim_matches(['\"', '\''])))
            .map(|value| match value {
                "usage" => StreamingMode::Usage,
                "block" => StreamingMode::Block,
                _ => StreamingMode::Rewrite,
            })
            .unwrap_or_default();
        true
    }

    /// 为每个请求创建独立的响应流计数上下文。
    fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(StreamingHttp {
            state_key: self.state_key.clone(),
            mode: self.mode,
            callbacks: 0,
            eof_callbacks: 0,
            usage_total: 0,
            blocked: false,
        }))
    }

    /// 声明该 root 创建 HTTP context。
    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }
}

/// 保存一次 HTTP 响应流的回调统计。
struct StreamingHttp {
    /// Host 测试读取状态时使用的 shared-data key。
    state_key: String,
    /// 当前测试选择的 Hai 语义模式。
    mode: StreamingMode,
    /// 已收到的 response-body 回调次数。
    callbacks: u32,
    /// `end_of_stream=true` 的终止回调次数。
    eof_callbacks: u32,
    /// 从多个 SSE chunk 累计的 usage 值。
    usage_total: u64,
    /// 是否曾过滤敏感 SSE chunk。
    blocked: bool,
}

impl Context for StreamingHttp {}

impl HttpContext for StreamingHttp {
    /// 改写当前 SSE chunk，并发布回调总数与 EOF 次数。
    fn on_http_response_body(&mut self, body_size: usize, end_of_stream: bool) -> Action {
        self.callbacks += 1;
        if end_of_stream {
            self.eof_callbacks += 1;
        }

        if body_size > 0 {
            let body = self.get_http_response_body(0, body_size).unwrap_or_default();
            if matches!(self.mode, StreamingMode::Usage) {
                self.usage_total += parse_usage(&body);
            }
            if matches!(self.mode, StreamingMode::Block) && body.windows(b"blocked".len()).any(|window| window == b"blocked") {
                self.blocked = true;
                self.set_http_response_body(0, body_size, b"");
            } else {
                let rewritten = prefix_sse_payloads(&body);
                self.set_http_response_body(0, body_size, &rewritten);
            }
        }

        let state = if end_of_stream && matches!(self.mode, StreamingMode::Usage) {
            format!("callbacks={};eof={};usage={}", self.callbacks, self.eof_callbacks, self.usage_total)
        } else if matches!(self.mode, StreamingMode::Block) {
            format!("callbacks={};eof={};blocked={}", self.callbacks, self.eof_callbacks, u8::from(self.blocked))
        } else {
            format!("callbacks={};eof={}", self.callbacks, self.eof_callbacks)
        };
        let _ = hostcalls::set_shared_data(&self.state_key, Some(state.as_bytes()), None);
        Action::Continue
    }
}

/// 提取测试 SSE chunk 中 `usage=<number>` 的数值。
fn parse_usage(body: &[u8]) -> u64 {
    let text = String::from_utf8_lossy(body);
    text.split("usage=")
        .skip(1)
        .filter_map(|tail| tail.chars().take_while(|ch| ch.is_ascii_digit()).collect::<String>().parse::<u64>().ok())
        .sum()
}

/// 为每一行 SSE `data:` 字段的 payload 添加 `filtered:` 前缀。
fn prefix_sse_payloads(body: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(body);
    let mut rewritten = String::with_capacity(text.len() + 16);
    for line in text.split_inclusive('\n') {
        if let Some(payload) = line.strip_prefix("data: ") {
            rewritten.push_str("data: filtered:");
            rewritten.push_str(payload);
        } else if let Some(payload) = line.strip_prefix("data:") {
            rewritten.push_str("data: filtered:");
            rewritten.push_str(payload);
        } else {
            rewritten.push_str(line);
        }
    }
    rewritten.into_bytes()
}
