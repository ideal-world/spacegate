//! `Vm`：单个 wasm 实例 + per-request 驱动状态机。
//!
//! 一个 `Vm` 包含：
//!
//! - `wasmtime::Store<HostState>`：宿主状态 + linear memory
//! - `wasmtime::Instance`：实例化后的 hai-process-mix
//! - 缓存的 guest exports（避免每次按名查找）
//!
//! 关键流程（[`Vm::process`]）：
//!
//! 1. `proxy_on_context_create(http_ctx_id, root_id)`
//! 2. `proxy_on_request_headers` → 解析 Action
//! 3. 若 Pause → `drive_until_continue`：循环 await dispatch_http_call 结果 →
//!    调 `proxy_on_http_call_response` → 直至 guest `continue_stream(Request)` 或 `send_local_response`
//! 4. 若 guest 写了 `local_response`：直接返回它（短路 inner.call）
//! 5. 否则：把 ctx 内的 headers 同步回 `SgRequest`，调 `inner.call`
//! 6. `proxy_on_response_headers` / `proxy_on_response_body` / `proxy_on_log`
//! 7. ctx 清理；vm 归池

use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue};
use http_body_util::BodyExt;
use spacegate_kernel::{SgBody, SgRequest, SgResponse};
use tracing::{debug, info, warn};
use wasmtime::{AsContextMut, Instance, Linker, Store, TypedFunc};

use crate::abi::Action;
use crate::config::{FailStrategy, WasmPluginShellConfig};
use crate::engine::shared_engine;
use crate::error::WasmHostError;
use crate::host_fn::register_all;
use crate::host_state::{ContextStage, HostState, HttpCallResult, PseudoHeaders, RequestContext};

/// 一次性 Vm（每次 plugin 调用都新建；首版不做池）。
pub struct Vm {
    store: Store<HostState>,
    #[allow(dead_code)]
    instance: Instance,
    root_id: u32,
    next_ctx_id: u32,
    dispatch_rx: tokio::sync::mpsc::UnboundedReceiver<(u32, HttpCallResult)>,
    fail_strategy: FailStrategy,
    fn_on_context_create: TypedFunc<(u32, u32), ()>,
    fn_on_vm_start: Option<TypedFunc<(u32, u32), u32>>,
    fn_on_configure: TypedFunc<(u32, u32), u32>,
    fn_on_request_headers: TypedFunc<(u32, u32, u32), u32>,
    fn_on_response_headers: TypedFunc<(u32, u32, u32), u32>,
    fn_on_response_body: Option<TypedFunc<(u32, u32, u32), u32>>,
    fn_on_http_call_response: TypedFunc<(u32, u32, u32, u32, u32), ()>,
    fn_on_log: Option<TypedFunc<u32, ()>>,
    fn_on_done: Option<TypedFunc<u32, u32>>,
    fn_on_delete: Option<TypedFunc<u32, ()>>,
}

impl Vm {
    /// 创建并启动一个 Vm：实例化 → 缓存 exports → 跑 vm_start/configure。
    pub async fn new(module: &wasmtime::Module, shell_cfg: Arc<WasmPluginShellConfig>) -> Result<Self, WasmHostError> {
        let engine = shared_engine();
        let host = HostState::new(shell_cfg.clone());
        let mut store: Store<HostState> = Store::new(engine, host);
        let mut linker: Linker<HostState> = Linker::new(engine);
        let (dispatch_tx, dispatch_rx) = tokio::sync::mpsc::unbounded_channel::<(u32, HttpCallResult)>();
        register_all(&mut linker, dispatch_tx).map_err(|e| WasmHostError::Instantiate(format!("register host fn: {e}")))?;

        // hai_process_mix 是 wasi reactor（_initialize export），但它 imports
        // `wasi_snapshot_preview1` 的 environ_get / fd_write / proc_exit / random_get
        // 等。我们用占位实现，给基础语义即可——hai 实际只在 log/random/clock 处依赖。
        register_wasi_stubs(&mut linker)?;

        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|e| WasmHostError::Instantiate(format!("instantiate: {e}")))?;

        // 拿 memory + alloc
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| WasmHostError::AbiViolation("no `memory` export".into()))?;
        store.data_mut().memory = Some(memory);
        if let Ok(alloc) = instance.get_typed_func::<u32, u32>(&mut store, "proxy_on_memory_allocate") {
            store.data_mut().alloc = Some(alloc);
        } else {
            return Err(WasmHostError::AbiViolation("no `proxy_on_memory_allocate` export".into()));
        }

        // 先跑 `_initialize`（wasi reactor）
        if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
            init.call(&mut store, ()).map_err(|e| WasmHostError::Instantiate(format!("_initialize: {e}")))?;
        }

        // 缓存其它 exports
        let fn_on_context_create = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, "proxy_on_context_create")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_context_create: {e}")))?;
        let fn_on_vm_start = instance.get_typed_func::<(u32, u32), u32>(&mut store, "proxy_on_vm_start").ok();
        let fn_on_configure = instance
            .get_typed_func::<(u32, u32), u32>(&mut store, "proxy_on_configure")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_configure: {e}")))?;
        let fn_on_request_headers = instance
            .get_typed_func::<(u32, u32, u32), u32>(&mut store, "proxy_on_request_headers")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_request_headers: {e}")))?;
        let fn_on_response_headers = instance
            .get_typed_func::<(u32, u32, u32), u32>(&mut store, "proxy_on_response_headers")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_response_headers: {e}")))?;
        let fn_on_response_body = instance.get_typed_func::<(u32, u32, u32), u32>(&mut store, "proxy_on_response_body").ok();
        let fn_on_http_call_response = instance
            .get_typed_func::<(u32, u32, u32, u32, u32), ()>(&mut store, "proxy_on_http_call_response")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_http_call_response: {e}")))?;
        let fn_on_log = instance.get_typed_func::<u32, ()>(&mut store, "proxy_on_log").ok();
        let fn_on_done = instance.get_typed_func::<u32, u32>(&mut store, "proxy_on_done").ok();
        let fn_on_delete = instance.get_typed_func::<u32, ()>(&mut store, "proxy_on_delete").ok();

        let root_id = store.data().root_context_id;
        let next_ctx_id = root_id + 1;
        let fail_strategy = shell_cfg.fail_strategy;

        let mut vm = Self {
            store,
            instance,
            root_id,
            next_ctx_id,
            dispatch_rx,
            fail_strategy,
            fn_on_context_create,
            fn_on_vm_start,
            fn_on_configure,
            fn_on_request_headers,
            fn_on_response_headers,
            fn_on_response_body,
            fn_on_http_call_response,
            fn_on_log,
            fn_on_done,
            fn_on_delete,
        };

        // 启动序：on_context_create(root, 0) → on_vm_start → on_configure
        vm.store.data_mut().contexts.insert(root_id, RequestContext::default());
        vm.create_context(root_id, 0)?;
        if let Some(ref f) = vm.fn_on_vm_start {
            vm.store.data_mut().effective_context = root_id;
            let cfg_len = vm.store.data().configuration.len() as u32;
            f.call(&mut vm.store, (root_id, cfg_len))
                .map_err(|e| WasmHostError::GuestTrap { hook: "on_vm_start", source: e })?;
        }
        vm.store.data_mut().effective_context = root_id;
        let cfg_len = vm.store.data().configuration.len() as u32;
        tracing::info!(target: "spacegate_plugin_wasm", cfg_len, "calling proxy_on_configure");
        let configure_fn = vm.fn_on_configure.clone();
        let ok = configure_fn
            .call(&mut vm.store, (root_id, cfg_len))
            .map_err(|e| WasmHostError::GuestTrap { hook: "on_configure", source: e })?;
        if ok == 0 {
            warn!(target: "spacegate_plugin_wasm", "guest on_configure returned 0 (=invalid config)");
        }
        Ok(vm)
    }

    fn create_context(&mut self, ctx_id: u32, parent_id: u32) -> Result<(), WasmHostError> {
        self.store.data_mut().effective_context = ctx_id;
        let f = self.fn_on_context_create.clone();
        f.call(&mut self.store, (ctx_id, parent_id))
            .map_err(|e| WasmHostError::GuestTrap { hook: "on_context_create", source: e })?;
        Ok(())
    }

    /// 完整跑一遍：on_request_headers → 可能多次 dispatch → inner.call → on_response_*
    pub async fn process(&mut self, req: SgRequest, inner: spacegate_plugin::Inner) -> Result<SgResponse, WasmHostError> {
        let http_ctx_id = self.next_ctx_id;
        self.next_ctx_id = self.next_ctx_id.wrapping_add(1);

        // 把请求拆出来：pseudo headers + headers，存进 ctx 之前需要把数据全部 clone 出来
        let (parts, body) = req.into_parts();
        let method = parts.method.clone();
        let uri = parts.uri.clone();
        let version = parts.version;
        let path = uri.path_and_query().map(|p| p.to_string()).unwrap_or_else(|| "/".to_string());
        let authority = uri.authority().map(|a| a.to_string()).unwrap_or_else(|| {
            parts
                .headers
                .get(http::header::HOST)
                .and_then(|h| h.to_str().ok())
                .unwrap_or("")
                .to_string()
        });
        let scheme = uri.scheme_str().unwrap_or("http").to_string();
        let mut headers = parts.headers.clone();
        // host 后续要根据 ctx 修改后写回，所以这份是 host 真实状态
        let pseudo = PseudoHeaders {
            method: method.as_str().to_string(),
            path: path.clone(),
            authority: authority.clone(),
            scheme,
        };

        // 创建 http context
        let root_id = self.root_id;
        self.create_context(http_ctx_id, root_id)?;
        {
            let st = self.store.data_mut();
            let ctx = st.contexts.entry(http_ctx_id).or_default();
            ctx.parent_id = root_id;
            ctx.stage = ContextStage::RequestHeaders;
            ctx.request_pseudo = pseudo;
            ctx.request_headers = headers.clone();
            ctx.continue_requested = false;
            st.effective_context = http_ctx_id;
        }

        // 调 on_request_headers
        let num_headers = (self.store.data().contexts[&http_ctx_id].request_headers.len() + 4) as u32; // +4 for pseudo
        let on_req_hdr = self.fn_on_request_headers.clone();
        let action_raw = on_req_hdr
            .call(&mut self.store, (http_ctx_id, num_headers, 1 /* end_of_stream=true 简化处理 */))
            .map_err(|e| WasmHostError::GuestTrap { hook: "on_request_headers", source: e })?;
        let action = Action::from_u32(action_raw);
        debug!(target: "spacegate_plugin_wasm", http_ctx_id, ?action, "on_request_headers returned");

        // 处理 Pause：等异步回调
        if action == Action::Pause {
            self.drive_until_continue(http_ctx_id).await?;
        }
        // 主流程：driver_until_continue 内部仍是 async（等 mpsc），保留 await。

        // 检查 local_response
        if let Some(local) = self.store.data_mut().contexts.get_mut(&http_ctx_id).and_then(|c| c.local_response.take()) {
            info!(target: "spacegate_plugin_wasm", http_ctx_id, status = local.status, "guest local response");
            // 调 on_log + on_done + on_delete
            self.invoke_log_done_delete(http_ctx_id);
            return Ok(build_local_response(local));
        }

        // 把 ctx 内的 headers 写回 SgRequest
        let new_headers = {
            let ctx = self.store.data().contexts.get(&http_ctx_id);
            ctx.map(|c| (c.request_headers.clone(), c.request_pseudo.clone())).unwrap_or_else(|| (HeaderMap::new(), PseudoHeaders::default()))
        };
        headers = new_headers.0;
        // 写回 host 真实状态：保留 method、重建 uri（path 可能被 guest 改）
        let new_uri = rebuild_uri(&new_headers.1.scheme, &new_headers.1.authority, &new_headers.1.path).unwrap_or(uri);
        let mut new_parts = parts;
        new_parts.method = new_headers.1.method.parse().unwrap_or(method);
        new_parts.uri = new_uri;
        new_parts.headers = headers;
        new_parts.version = version;
        let new_req = SgRequest::from_parts(new_parts, body);

        // inner.call
        let resp = inner.call(new_req).await;

        // on_response_headers
        let (resp_parts, resp_body) = resp.into_parts();
        let status = resp_parts.status.as_u16();
        let resp_headers = resp_parts.headers.clone();
        {
            let st = self.store.data_mut();
            if let Some(ctx) = st.contexts.get_mut(&http_ctx_id) {
                ctx.stage = ContextStage::ResponseHeaders;
                ctx.response_status = Some(status);
                ctx.response_headers = resp_headers.clone();
                ctx.continue_requested = false;
                st.effective_context = http_ctx_id;
            }
        }
        let on_resp_hdr = self.fn_on_response_headers.clone();
        let _ = on_resp_hdr
            .call(&mut self.store, (http_ctx_id, (resp_headers.len() + 1) as u32, 1))
            .map_err(|e| WasmHostError::GuestTrap { hook: "on_response_headers", source: e })?;

        // on_response_body：把 body dump 一次喂给 guest（首版非流式）
        let on_resp_body = self.fn_on_response_body.clone();
        let (final_headers, final_body): (HeaderMap, SgBody) = if let Some(f) = on_resp_body {
            let collected = match resp_body.collect().await {
                Ok(c) => c.to_bytes(),
                Err(_) => Bytes::new(),
            };
            let body_size = collected.len() as u32;
            {
                let st = self.store.data_mut();
                if let Some(ctx) = st.contexts.get_mut(&http_ctx_id) {
                    ctx.response_body = Some(collected.clone());
                    ctx.stage = ContextStage::ResponseBody;
                    st.effective_context = http_ctx_id;
                }
            }
            let _ = f
                .call(&mut self.store, (http_ctx_id, body_size, 1))
                .map_err(|e| WasmHostError::GuestTrap { hook: "on_response_body", source: e })?;
            // 取回（guest 可能改过）
            let updated_body = self.store.data().contexts.get(&http_ctx_id).and_then(|c| c.response_body.clone()).unwrap_or(collected);
            let updated_headers = self.store.data().contexts.get(&http_ctx_id).map(|c| c.response_headers.clone()).unwrap_or(resp_headers);
            (updated_headers, SgBody::full(updated_body))
        } else {
            (resp_headers, SgBody::new(resp_body))
        };

        // on_log + on_done + on_delete
        self.invoke_log_done_delete(http_ctx_id);

        let mut new_resp_parts = resp_parts;
        new_resp_parts.headers = final_headers;
        Ok(SgResponse::from_parts(new_resp_parts, final_body))
    }

    /// 在 guest 返回 Pause 之后，不停地 await dispatch_rx 来驱动状态机，
    /// 直到 guest `continue_stream(Request)` 或写了 `local_response`。
    async fn drive_until_continue(&mut self, ctx_id: u32) -> Result<(), WasmHostError> {
        loop {
            // 退出条件
            {
                let st = self.store.data();
                let Some(ctx) = st.contexts.get(&ctx_id) else {
                    return Err(WasmHostError::AbiViolation(format!("ctx {ctx_id} gone")));
                };
                if ctx.local_response.is_some() {
                    return Ok(());
                }
                if ctx.continue_requested && st.pending_calls.is_empty() {
                    return Ok(());
                }
            }
            // 等下一个 dispatch 完成
            let Some((token, result)) = self.dispatch_rx.recv().await else {
                return Err(WasmHostError::Dispatch("dispatch channel closed".to_string()));
            };
            let source_ctx_id = self
                .store
                .data_mut()
                .pending_calls
                .remove(&token)
                .map(|p| p.source_context_id)
                .unwrap_or(ctx_id);
            let header_count;
            let body_len;
            {
                let st = self.store.data_mut();
                st.effective_context = source_ctx_id;
                if let Some(ctx) = st.contexts.get_mut(&source_ctx_id) {
                    ctx.last_call_headers = result.headers.clone();
                    // 单独存 status：HeaderMap 不接受 `:` key，pseudo_lookup 会读这里。
                    ctx.last_call_status = result.status;
                    ctx.last_call_body = result.body.clone();
                    ctx.continue_requested = false;
                }
                header_count = result.headers.len() as u32 + 1;
                body_len = result.body.len() as u32;
            }
            debug!(target: "spacegate_plugin_wasm", token, source_ctx_id, status = result.status, body_len, "fire proxy_on_http_call_response");
            // 注意：proxy_on_http_call_response 通过 host fn 读取 last_call_*；
            // 但 hai 通过 `get_http_call_response_header(":status")` 读 status：
            // 我们 lookup_header 时对 HttpCallResponseHeaders 的 `:status` 做特判
            let f = self.fn_on_http_call_response.clone();
            f.call(&mut self.store, (source_ctx_id, token, header_count, body_len, 0))
                .map_err(|e| WasmHostError::GuestTrap { hook: "on_http_call_response", source: e })?;
        }
    }

    fn invoke_log_done_delete(&mut self, ctx_id: u32) {
        self.store.data_mut().effective_context = ctx_id;
        if let Some(f) = self.fn_on_log.clone() {
            let _ = f.call(&mut self.store, ctx_id);
        }
        if let Some(f) = self.fn_on_done.clone() {
            let _ = f.call(&mut self.store, ctx_id);
        }
        if let Some(f) = self.fn_on_delete.clone() {
            let _ = f.call(&mut self.store, ctx_id);
        }
        // 清理 ctx
        self.store.data_mut().contexts.remove(&ctx_id);
    }

    pub fn fail_strategy(&self) -> FailStrategy {
        self.fail_strategy
    }
}

fn rebuild_uri(scheme: &str, authority: &str, path: &str) -> Option<http::Uri> {
    let mut s = String::new();
    if !scheme.is_empty() && !authority.is_empty() {
        s.push_str(scheme);
        s.push_str("://");
        s.push_str(authority);
    }
    if !path.is_empty() {
        s.push_str(path);
    } else {
        s.push('/');
    }
    s.parse().ok()
}

fn build_local_response(local: crate::host_state::LocalResponse) -> SgResponse {
    let mut resp = SgResponse::new(SgBody::full(local.body));
    *resp.status_mut() = http::StatusCode::from_u16(local.status).unwrap_or(http::StatusCode::OK);
    for (k, v) in local.headers.iter() {
        resp.headers_mut().insert(k, v.clone());
    }
    resp
}

/// 占位的 wasi_snapshot_preview1 hostcall：满足 hai_process_mix 的 _initialize 链接需求。
///
/// - random_get / clock_time_get：用 host 端真实实现
/// - environ_get / environ_sizes_get / fd_write / proc_exit：写到日志或返回 0
fn register_wasi_stubs(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // random_get(ptr, len) -> errno
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "random_get",
        |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
            let mem = match crate::abi::MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return 1,
            };
            let mut buf = vec![0u8; len.max(0) as usize];
            // 简化：使用 SystemTime 的 nanos 做种子异或填充；不是密码学安全，对 hai 足够（hai 几乎不用 random）
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            for (i, b) in buf.iter_mut().enumerate() {
                *b = ((seed >> (i % 56)) as u8) ^ (i as u8);
            }
            let _ = mem.write_bytes(caller.as_context_mut(), ptr as u32, &buf);
            0
        },
    )?;
    // clock_time_get(clock_id, precision, *result) -> errno
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "clock_time_get",
        |mut caller: wasmtime::Caller<'_, HostState>, _clock_id: i32, _prec: i64, return_ptr: i32| -> i32 {
            let mem = match crate::abi::MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return 1,
            };
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let _ = mem.write_u64(caller.as_context_mut(), return_ptr as u32, nanos);
            0
        },
    )?;
    // environ_get(*environ, *environ_buf) -> errno
    linker.func_wrap("wasi_snapshot_preview1", "environ_get", |_c: wasmtime::Caller<'_, HostState>, _a: i32, _b: i32| -> i32 { 0 })?;
    // environ_sizes_get(*environc, *environ_buf_size) -> errno
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "environ_sizes_get",
        |mut caller: wasmtime::Caller<'_, HostState>, count_ptr: i32, buf_ptr: i32| -> i32 {
            let mem = match crate::abi::MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return 1,
            };
            let _ = mem.write_u32(caller.as_context_mut(), count_ptr as u32, 0);
            let _ = mem.write_u32(caller.as_context_mut(), buf_ptr as u32, 0);
            0
        },
    )?;
    // fd_write(fd, *iovs, iovs_len, *nwritten) -> errno —— hai 用 println 时会走这条，简单丢弃
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_write",
        |mut caller: wasmtime::Caller<'_, HostState>, _fd: i32, _iovs: i32, _iovs_len: i32, nwritten_ptr: i32| -> i32 {
            let mem = match crate::abi::MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return 1,
            };
            let _ = mem.write_u32(caller.as_context_mut(), nwritten_ptr as u32, 0);
            0
        },
    )?;
    linker.func_wrap("wasi_snapshot_preview1", "proc_exit", |_c: wasmtime::Caller<'_, HostState>, _code: i32| {})?;
    Ok(())
}
