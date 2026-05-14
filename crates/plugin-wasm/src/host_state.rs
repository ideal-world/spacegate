//! 传给 `wasmtime::Store<T>` 的宿主状态。
//!
//! - 顶层 [`HostState`] 承载：进程级 reqwest 客户端、shell 配置、序列化后的 plugin_config 字节、
//!   memory / 分配器 export、所有 HTTP 上下文、未完结的 `proxy_http_call` 句柄等。
//! - 每个 HTTP 请求建一个 [`RequestContext`]，由 `vm.rs` 在调 `proxy_on_*` 钩子前后维护。
//! - host fn 通过 `caller.data() / data_mut()` 读写 `HostState`，并以
//!   `effective_context` 字段定位「当前是哪个上下文」（spec §Effective context changes，
//!   guest 通过 `proxy_set_effective_context` 切换）。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;
use wasmtime::{Memory, TypedFunc};

use crate::config::WasmPluginShellConfig;

/// 约定的 root context id：proxy-wasm 默认从 1 开始。
pub const ROOT_CONTEXT_ID: u32 = 1;

/// HTTP 上下文在生命周期中处于的阶段（vm.rs 调钩子时打标记，host fn 据此判断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextStage {
    #[default]
    Init,
    RequestHeaders,
    RequestBody,
    RequestTrailers,
    ResponseHeaders,
    ResponseBody,
    ResponseTrailers,
    Log,
}

/// HTTP/2 风格的伪头（`:method` 等）。proxy-wasm guest 通过 header_map 拿到它们，
/// 我们额外用一个结构体专门承载，便于在 `inner.call` 前重建 `Uri`。
#[derive(Debug, Clone, Default)]
pub struct PseudoHeaders {
    pub method: String,
    pub path: String,
    pub authority: String,
    pub scheme: String,
}

/// guest 调 `proxy_send_local_response` 时 host 捕获的结构。
#[derive(Debug)]
pub struct LocalResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
}

/// `proxy_http_call` 的异步结果：spawn 出去的 reqwest 任务通过 channel 把它送回 Vm。
#[derive(Debug, Default)]
pub struct HttpCallResult {
    pub status: u16,
    pub status_message: String,
    pub headers: HeaderMap,
    pub body: Bytes,
}

/// 一次未完结的 `proxy_http_call`。`source_context_id` 指明该 token 是哪个 ctx 发起的，
/// 这样 Vm 状态机在拿到结果时能恢复到正确的 effective_context 再调 `proxy_on_http_call_response`。
#[derive(Debug)]
pub struct PendingCall {
    #[allow(dead_code)]
    pub waker: Option<std::task::Waker>,
    pub source_context_id: u32,
}

/// 单个 HTTP 请求的所有状态（请求/响应头 / body / 上次 dispatch 结果 / 本地响应 / 短路标记）。
#[derive(Debug, Default)]
pub struct RequestContext {
    pub parent_id: u32,
    pub stage: ContextStage,
    pub request_pseudo: PseudoHeaders,
    pub request_headers: HeaderMap,
    pub request_trailers: HeaderMap,
    pub request_body: Option<Bytes>,
    pub response_status: Option<u16>,
    pub response_status_message: String,
    pub response_headers: HeaderMap,
    pub response_trailers: HeaderMap,
    pub response_body: Option<Bytes>,
    /// 上次 `proxy_http_call` 回调时由 host 注入；guest 通过
    /// `get_http_call_response_*` 读它。
    pub last_call_headers: HeaderMap,
    pub last_call_trailers: HeaderMap,
    pub last_call_body: Bytes,
    /// 最近一次 dispatch_http_call 返回的状态码（hai 用 `:status` 伪头读取）。
    pub last_call_status: u16,
    pub last_call_status_message: String,
    /// guest 显式 `resume_http_request()` 后置 true；Vm 退出 Pause 等待循环。
    pub continue_requested: bool,
    /// guest 调 `send_local_response` 后写入；Vm 据此短路返回。
    pub local_response: Option<LocalResponse>,
    /// HTTP 协议版本字符串（spec well-known property `request.protocol`）。
    pub request_protocol: String,
    /// 收到的 request body 已知字节数。
    pub request_size: u64,
    /// 输出的 response body 已知字节数。
    pub response_size: u64,
    /// 通过 `proxy_done` 显式标记的 done 阶段（spec §proxy_done / §proxy_on_done）。
    pub done_marker: bool,
    /// guest 上一次 `proxy_on_done` 返回值；false 表示要等 `proxy_done` 才能进 on_log/on_delete。
    pub awaiting_done: bool,
}

/// 进程内传给 wasmtime `Store` 的状态。生命周期与一次 Vm 实例一致。
///
/// 不 derive Debug 因为 `TypedFunc` 不实现 Debug。
pub struct HostState {
    pub shell_cfg: Arc<WasmPluginShellConfig>,
    /// guest `proxy_on_configure` 读取的字节（来自 shell_cfg.plugin_config 序列化）。
    pub configuration: Vec<u8>,
    /// guest 导出的线性内存（vm.rs 实例化完成后填）。
    pub memory: Option<Memory>,
    /// guest 导出 `proxy_on_memory_allocate(size) -> ptr` 或 deprecated `malloc(size) -> ptr`。
    pub alloc: Option<TypedFunc<u32, u32>>,
    pub root_context_id: u32,
    /// 当前 hostcall 关联的上下文 id（由 vm.rs 在每次钩子前设置，
    /// 也可被 guest 的 `proxy_set_effective_context` 覆盖）。
    pub effective_context: u32,
    pub contexts: HashMap<u32, RequestContext>,
    /// guest 调用 `proxy_set_tick_period_milliseconds` 后存这里。
    /// `WasmPluginShell` 的后台 tick 任务 50ms 颗粒度地轮询本字段，到点 → `Vm::tick()`。
    pub tick_period_ms: Option<u32>,
    /// 未完结的 dispatch_http_call 句柄表。
    pub pending_calls: HashMap<u32, PendingCall>,
    /// dispatch token 单调递增计数器。
    next_token: u32,
    /// host 端 reqwest 客户端：所有 dispatch_http_call 复用一个，免去握手开销。
    pub http_client: reqwest::Client,
    /// 用户通过 `proxy_set_property` 设置的自定义属性（key = `\0` 分割的 path 字节）。
    pub user_properties: HashMap<Vec<u8>, Vec<u8>>,
    /// 客户端 socket 地址（spec well-known property `source.address` / `source.port`）。
    pub source_addr: Option<SocketAddr>,
    /// 服务端 socket 地址（spec well-known property `destination.address` / `destination.port`）。
    pub destination_addr: Option<SocketAddr>,
    /// 插件标识（spec well-known property `plugin_name` / `plugin_root_id` / `plugin_vm_id`）。
    pub plugin_name: String,
    pub plugin_root_id: String,
    pub plugin_vm_id: String,
}

impl HostState {
    pub fn new(shell_cfg: Arc<WasmPluginShellConfig>) -> Self {
        let configuration = shell_cfg.configuration_bytes();
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(8)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let plugin_name = shell_cfg.plugin_name.clone();
        let plugin_root_id = shell_cfg.plugin_root_id.clone();
        let plugin_vm_id = shell_cfg.plugin_vm_id.clone();
        Self {
            shell_cfg,
            configuration,
            memory: None,
            alloc: None,
            root_context_id: ROOT_CONTEXT_ID,
            effective_context: ROOT_CONTEXT_ID,
            contexts: HashMap::new(),
            tick_period_ms: None,
            pending_calls: HashMap::new(),
            next_token: 1,
            http_client,
            user_properties: HashMap::new(),
            source_addr: None,
            destination_addr: None,
            plugin_name,
            plugin_root_id,
            plugin_vm_id,
        }
    }

    /// 取当前生效的 ctx 的不可变引用（host fn 大量使用）。
    pub fn current_context(&self) -> Option<&RequestContext> {
        self.contexts.get(&self.effective_context)
    }

    /// 取当前生效的 ctx 的可变引用。
    #[allow(dead_code)]
    pub fn current_context_mut(&mut self) -> Option<&mut RequestContext> {
        self.contexts.get_mut(&self.effective_context)
    }

    /// 分配下一个 dispatch_http_call token；约定 0 保留，token 从 1 开始单调递增。
    pub fn next_dispatch_token(&mut self) -> u32 {
        let t = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        t
    }
}
