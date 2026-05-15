//! `Vm`：单个 wasm 实例 + per-request 驱动状态机。
//!
//! 一个 `Vm` 包含：
//!
//! - `wasmtime::Store<HostState>`：宿主状态 + linear memory
//! - `wasmtime::Instance`：实例化后的 wasm guest
//! - 缓存的 guest exports（避免每次按名查找）
//!
//! 关键流程（[`Vm::process`]）：
//!
//! 1. `proxy_on_context_create(http_ctx_id, root_id)`
//! 2. `proxy_on_request_headers` → 解析 Action（end_of_stream=false 当有 body 时）
//! 3. 若 Pause → `drive_until_continue`：循环 await dispatch_http_call 结果 →
//!    调 `proxy_on_http_call_response` → 直至 guest `continue_stream(HTTP_REQUEST)` 或 `send_local_response`
//! 4. 若 guest 导出 `proxy_on_request_body`：收齐 body，调 hook，可能再次 Pause；body 写回 SgRequest
//! 5. 若 guest 导出 `proxy_on_request_trailers`：用空 trailer map 调一次（spacegate 暂不暴露 trailer）
//! 6. 若 guest 写了 `local_response`：直接返回它（短路 inner.call）
//! 7. 否则：把 ctx 内的 headers/body 同步回 `SgRequest`，调 `inner.call`
//! 8. `proxy_on_response_headers` / `proxy_on_response_body` / `proxy_on_response_trailers` / `proxy_on_log` /
//!    `proxy_on_done` (spec 要求 false 时等 `proxy_done`) / `proxy_on_delete`
//! 9. ctx 清理

use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;
use http_body_util::BodyExt;
use spacegate_kernel::{SgBody, SgRequest, SgResponse};
use tracing::{debug, info, warn};
use wasmtime::{AsContext, AsContextMut, Instance, Linker, Store, TypedFunc};

use crate::abi::{Action, MemoryHelper};
use crate::config::{FailStrategy, WasmPluginShellConfig};
use crate::engine::shared_engine;
use crate::error::WasmHostError;
use crate::host_fn::register_all;
use crate::host_state::{ContextStage, HostState, HttpCallResult, PseudoHeaders, RequestContext};

/// 长生命 Vm：插件 `create` 时实例化一次，之后被多次请求复用，再加一条
/// 后台 tick 任务用来驱动 `proxy_on_tick`。`store` !Sync，所以共享时必须
/// 套 `tokio::sync::Mutex`（见 `shell.rs`）。
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
    fn_on_request_body: Option<TypedFunc<(u32, u32, u32), u32>>,
    fn_on_request_trailers: Option<TypedFunc<(u32, u32), u32>>,
    fn_on_response_headers: TypedFunc<(u32, u32, u32), u32>,
    fn_on_response_body: Option<TypedFunc<(u32, u32, u32), u32>>,
    fn_on_response_trailers: Option<TypedFunc<(u32, u32), u32>>,
    fn_on_http_call_response: TypedFunc<(u32, u32, u32, u32, u32), ()>,
    fn_on_log: Option<TypedFunc<u32, ()>>,
    fn_on_done: Option<TypedFunc<u32, u32>>,
    fn_on_delete: Option<TypedFunc<u32, ()>>,
    fn_on_tick: Option<TypedFunc<u32, ()>>,
}

impl Vm {
    /// 创建并启动一个 Vm：实例化 → 缓存 exports → 跑 vm_start/configure。
    ///
    /// 这是同步函数。整个过程不涉及 `await`（wasmtime 编译/实例化、guest `_initialize`、
    /// `on_vm_start` / `on_configure` 全部是同步调用），所以 `WasmPluginShell::create`
    /// 这种 sync 上下文也能直接构造。
    pub fn new(module: &wasmtime::Module, shell_cfg: Arc<WasmPluginShellConfig>) -> Result<Self, WasmHostError> {
        let engine = shared_engine();
        let host = HostState::new(shell_cfg.clone());
        let mut store: Store<HostState> = Store::new(engine, host);
        let mut linker: Linker<HostState> = Linker::new(engine);
        let (dispatch_tx, dispatch_rx) = tokio::sync::mpsc::unbounded_channel::<(u32, HttpCallResult)>();
        register_all(&mut linker, dispatch_tx).map_err(|e| WasmHostError::Instantiate(format!("register host fn: {e}")))?;

        register_wasi_stubs(&mut linker)?;

        let instance = linker.instantiate(&mut store, module).map_err(|e| WasmHostError::Instantiate(format!("instantiate: {e}")))?;

        let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| WasmHostError::AbiViolation("no `memory` export".into()))?;
        store.data_mut().memory = Some(memory);
        // spec §Memory management：优先 `proxy_on_memory_allocate`，否则回退 `malloc`。
        if let Ok(alloc) = instance.get_typed_func::<u32, u32>(&mut store, "proxy_on_memory_allocate") {
            store.data_mut().alloc = Some(alloc);
        } else if let Ok(alloc) = instance.get_typed_func::<u32, u32>(&mut store, "malloc") {
            store.data_mut().alloc = Some(alloc);
        } else {
            return Err(WasmHostError::AbiViolation("no memory allocator export (proxy_on_memory_allocate or malloc)".into()));
        }

        // spec §Integration：先 `_initialize`；若不存在尝试 `_start`。
        if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
            init.call(&mut store, ()).map_err(|e| WasmHostError::Instantiate(format!("_initialize: {e}")))?;
        } else if let Ok(start) = instance.get_typed_func::<(), ()>(&mut store, "_start") {
            start.call(&mut store, ()).map_err(|e| WasmHostError::Instantiate(format!("_start: {e}")))?;
        }

        let fn_on_context_create = instance
            .get_typed_func::<(u32, u32), ()>(&mut store, "proxy_on_context_create")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_context_create: {e}")))?;
        let fn_on_vm_start = instance.get_typed_func::<(u32, u32), u32>(&mut store, "proxy_on_vm_start").ok();
        let fn_on_configure =
            instance.get_typed_func::<(u32, u32), u32>(&mut store, "proxy_on_configure").map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_configure: {e}")))?;
        let fn_on_request_headers = instance
            .get_typed_func::<(u32, u32, u32), u32>(&mut store, "proxy_on_request_headers")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_request_headers: {e}")))?;
        let fn_on_request_body = instance.get_typed_func::<(u32, u32, u32), u32>(&mut store, "proxy_on_request_body").ok();
        let fn_on_request_trailers = instance.get_typed_func::<(u32, u32), u32>(&mut store, "proxy_on_request_trailers").ok();
        let fn_on_response_headers = instance
            .get_typed_func::<(u32, u32, u32), u32>(&mut store, "proxy_on_response_headers")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_response_headers: {e}")))?;
        let fn_on_response_body = instance.get_typed_func::<(u32, u32, u32), u32>(&mut store, "proxy_on_response_body").ok();
        let fn_on_response_trailers = instance.get_typed_func::<(u32, u32), u32>(&mut store, "proxy_on_response_trailers").ok();
        let fn_on_http_call_response = instance
            .get_typed_func::<(u32, u32, u32, u32, u32), ()>(&mut store, "proxy_on_http_call_response")
            .map_err(|e| WasmHostError::AbiViolation(format!("get proxy_on_http_call_response: {e}")))?;
        let fn_on_log = instance.get_typed_func::<u32, ()>(&mut store, "proxy_on_log").ok();
        let fn_on_done = instance.get_typed_func::<u32, u32>(&mut store, "proxy_on_done").ok();
        let fn_on_delete = instance.get_typed_func::<u32, ()>(&mut store, "proxy_on_delete").ok();
        let fn_on_tick = instance.get_typed_func::<u32, ()>(&mut store, "proxy_on_tick").ok();

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
            fn_on_request_body,
            fn_on_request_trailers,
            fn_on_response_headers,
            fn_on_response_body,
            fn_on_response_trailers,
            fn_on_http_call_response,
            fn_on_log,
            fn_on_done,
            fn_on_delete,
            fn_on_tick,
        };

        // 启动序：on_context_create(root, 0) → on_vm_start → on_configure
        vm.store.data_mut().contexts.insert(root_id, RequestContext::default());
        vm.create_context(root_id, 0)?;
        if let Some(ref f) = vm.fn_on_vm_start {
            vm.store.data_mut().effective_context = root_id;
            let cfg_len = vm.store.data().configuration.len() as u32;
            let ok = f.call(&mut vm.store, (root_id, cfg_len)).map_err(|e| WasmHostError::GuestTrap { hook: "on_vm_start", source: e })?;
            if ok == 0 {
                return Err(WasmHostError::Instantiate("guest on_vm_start returned 0 (=invalid VM configuration)".into()));
            }
        }
        vm.store.data_mut().effective_context = root_id;
        let cfg_len = vm.store.data().configuration.len() as u32;
        tracing::info!(target: "spacegate_plugin_wasm", cfg_len, "calling proxy_on_configure");
        let configure_fn = vm.fn_on_configure.clone();
        let ok = configure_fn.call(&mut vm.store, (root_id, cfg_len)).map_err(|e| WasmHostError::GuestTrap { hook: "on_configure", source: e })?;
        if ok == 0 {
            warn!(target: "spacegate_plugin_wasm", "guest on_configure returned 0 (=invalid config)");
        }
        Ok(vm)
    }

    fn create_context(&mut self, ctx_id: u32, parent_id: u32) -> Result<(), WasmHostError> {
        self.store.data_mut().effective_context = ctx_id;
        let f = self.fn_on_context_create.clone();
        f.call(&mut self.store, (ctx_id, parent_id)).map_err(|e| WasmHostError::GuestTrap {
            hook: "on_context_create",
            source: e,
        })?;
        Ok(())
    }

    /// 完整跑一遍：on_request_headers → 可能多次 dispatch → on_request_body → inner.call → on_response_*
    pub async fn process(&mut self, req: SgRequest, inner: spacegate_plugin::Inner) -> Result<SgResponse, WasmHostError> {
        // 跨请求清理：上一次请求若提前 `send_local_response` 短路，可能留下未消费的
        // dispatch 结果和 pending token，不清掉会让本请求的 `drive_until_continue`
        // 把陈旧响应误当成自己的（spec §proxy_http_call 不要求 host 持久化）。
        while self.dispatch_rx.try_recv().is_ok() {}
        self.store.data_mut().pending_calls.clear();

        let http_ctx_id = self.next_ctx_id;
        self.next_ctx_id = self.next_ctx_id.wrapping_add(1);

        let (parts, body) = req.into_parts();
        let method = parts.method.clone();
        let uri = parts.uri.clone();
        let version = parts.version;
        let path = uri.path_and_query().map(|p| p.to_string()).unwrap_or_else(|| "/".to_string());
        let authority = uri.authority().map(|a| a.to_string()).unwrap_or_else(|| parts.headers.get(http::header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("").to_string());
        let scheme = uri.scheme_str().unwrap_or("http").to_string();
        let headers = parts.headers.clone();
        let pseudo = PseudoHeaders {
            method: method.as_str().to_string(),
            path: path.clone(),
            authority: authority.clone(),
            scheme,
        };
        let request_protocol = format!("{:?}", version);

        let want_request_body = self.fn_on_request_body.is_some();

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
            ctx.request_protocol = request_protocol;
            st.effective_context = http_ctx_id;
        }

        // 调 on_request_headers
        let num_headers = (self.store.data().contexts[&http_ctx_id].request_headers.len() + 4) as u32;
        let end_of_stream_for_headers: u32 = if want_request_body { 0 } else { 1 };
        let on_req_hdr = self.fn_on_request_headers.clone();
        let action_raw = on_req_hdr.call(&mut self.store, (http_ctx_id, num_headers, end_of_stream_for_headers)).map_err(|e| WasmHostError::GuestTrap {
            hook: "on_request_headers",
            source: e,
        })?;
        let action = Action::from_u32(action_raw);
        debug!(target: "spacegate_plugin_wasm", http_ctx_id, ?action, "on_request_headers returned");

        if action == Action::Pause {
            self.drive_until_continue(http_ctx_id).await?;
        }

        if let Some(local) = self.store.data_mut().contexts.get_mut(&http_ctx_id).and_then(|c| c.local_response.take()) {
            info!(target: "spacegate_plugin_wasm", http_ctx_id, status = local.status, "guest local response (after headers)");
            self.invoke_log_done_delete(http_ctx_id)?;
            return Ok(build_local_response(local));
        }

        // ─── on_request_body：把请求 body 物化后喂给 guest（仅当 guest 导出该 hook）───
        let (new_req_for_inner, collected_body_after_hook) = if want_request_body {
            // collect body
            let collected = match body.collect().await {
                Ok(c) => c.to_bytes(),
                Err(_) => Bytes::new(),
            };
            let body_size = collected.len() as u32;
            {
                let st = self.store.data_mut();
                if let Some(ctx) = st.contexts.get_mut(&http_ctx_id) {
                    ctx.request_body = Some(collected.clone());
                    ctx.stage = ContextStage::RequestBody;
                    ctx.continue_requested = false;
                    ctx.request_size = collected.len() as u64;
                    st.effective_context = http_ctx_id;
                }
            }
            let on_req_body = self.fn_on_request_body.clone().expect("guarded by want_request_body");
            let action_raw = on_req_body.call(&mut self.store, (http_ctx_id, body_size, 1)).map_err(|e| WasmHostError::GuestTrap {
                hook: "on_request_body",
                source: e,
            })?;
            if Action::from_u32(action_raw) == Action::Pause {
                self.drive_until_continue(http_ctx_id).await?;
            }
            if let Some(local) = self.store.data_mut().contexts.get_mut(&http_ctx_id).and_then(|c| c.local_response.take()) {
                info!(target: "spacegate_plugin_wasm", http_ctx_id, status = local.status, "guest local response (after request body)");
                self.invoke_log_done_delete(http_ctx_id)?;
                return Ok(build_local_response(local));
            }
            let final_body = self.store.data().contexts.get(&http_ctx_id).and_then(|c| c.request_body.clone()).unwrap_or(collected);
            (None, Some(final_body))
        } else {
            (Some(body), None)
        };

        // ─── on_request_trailers：spacegate 当前不感知 trailers，给 guest 一个空 trailer 入参 ───
        if let Some(f) = self.fn_on_request_trailers.clone() {
            self.store.data_mut().effective_context = http_ctx_id;
            if let Some(ctx) = self.store.data_mut().contexts.get_mut(&http_ctx_id) {
                ctx.stage = ContextStage::RequestTrailers;
                ctx.continue_requested = false;
            }
            let action_raw = f.call(&mut self.store, (http_ctx_id, 0)).map_err(|e| WasmHostError::GuestTrap {
                hook: "on_request_trailers",
                source: e,
            })?;
            if Action::from_u32(action_raw) == Action::Pause {
                self.drive_until_continue(http_ctx_id).await?;
            }
            if let Some(local) = self.store.data_mut().contexts.get_mut(&http_ctx_id).and_then(|c| c.local_response.take()) {
                info!(target: "spacegate_plugin_wasm", http_ctx_id, status = local.status, "guest local response (after request trailers)");
                self.invoke_log_done_delete(http_ctx_id)?;
                return Ok(build_local_response(local));
            }
        }

        // 把 ctx 内可能被 guest 改过的 method/path/headers 写回 SgRequest
        let (new_headers, new_pseudo) = self
            .store
            .data()
            .contexts
            .get(&http_ctx_id)
            .map(|c| (c.request_headers.clone(), c.request_pseudo.clone()))
            .unwrap_or_else(|| (HeaderMap::new(), PseudoHeaders::default()));
        let new_uri = rebuild_uri(&new_pseudo.scheme, &new_pseudo.authority, &new_pseudo.path).unwrap_or(uri);
        let mut new_parts = parts;
        new_parts.method = new_pseudo.method.parse().unwrap_or(method);
        new_parts.uri = new_uri;
        new_parts.headers = new_headers;
        new_parts.version = version;
        let new_body = match (new_req_for_inner, collected_body_after_hook) {
            (Some(b), _) => b,
            (None, Some(bytes)) => SgBody::full(bytes),
            (None, None) => SgBody::empty(),
        };
        let new_req = SgRequest::from_parts(new_parts, new_body);

        let resp = inner.call(new_req).await;

        // ─── on_response_headers ───
        let (resp_parts, resp_body) = resp.into_parts();
        let status = resp_parts.status.as_u16();
        let status_message = resp_parts.status.canonical_reason().unwrap_or("").to_string();
        let resp_headers = resp_parts.headers.clone();
        {
            let st = self.store.data_mut();
            if let Some(ctx) = st.contexts.get_mut(&http_ctx_id) {
                ctx.stage = ContextStage::ResponseHeaders;
                ctx.response_status = Some(status);
                ctx.response_status_message = status_message;
                ctx.response_headers = resp_headers.clone();
                ctx.continue_requested = false;
                st.effective_context = http_ctx_id;
            }
        }
        let want_response_body = self.fn_on_response_body.is_some();
        let end_of_stream_for_resp_hdr: u32 = if want_response_body { 0 } else { 1 };
        let on_resp_hdr = self.fn_on_response_headers.clone();
        let action_raw = on_resp_hdr.call(&mut self.store, (http_ctx_id, (resp_headers.len() + 1) as u32, end_of_stream_for_resp_hdr)).map_err(|e| WasmHostError::GuestTrap {
            hook: "on_response_headers",
            source: e,
        })?;
        if Action::from_u32(action_raw) == Action::Pause {
            self.drive_until_continue(http_ctx_id).await?;
        }
        if let Some(local) = self.store.data_mut().contexts.get_mut(&http_ctx_id).and_then(|c| c.local_response.take()) {
            info!(target: "spacegate_plugin_wasm", http_ctx_id, status = local.status, "guest local response (after response headers)");
            self.invoke_log_done_delete(http_ctx_id)?;
            return Ok(build_local_response(local));
        }

        // ─── on_response_body ───
        let (mut final_headers, final_body): (HeaderMap, SgBody) = if let Some(f) = self.fn_on_response_body.clone() {
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
                    ctx.continue_requested = false;
                    ctx.response_size = collected.len() as u64;
                    st.effective_context = http_ctx_id;
                }
            }
            let action_raw = f.call(&mut self.store, (http_ctx_id, body_size, 1)).map_err(|e| WasmHostError::GuestTrap {
                hook: "on_response_body",
                source: e,
            })?;
            if Action::from_u32(action_raw) == Action::Pause {
                self.drive_until_continue(http_ctx_id).await?;
            }
            let updated_body = self.store.data().contexts.get(&http_ctx_id).and_then(|c| c.response_body.clone()).unwrap_or(collected);
            let updated_headers = self.store.data().contexts.get(&http_ctx_id).map(|c| c.response_headers.clone()).unwrap_or(resp_headers);
            (updated_headers, SgBody::full(updated_body))
        } else {
            (resp_headers, SgBody::new(resp_body))
        };

        // ─── on_response_trailers ───
        if let Some(f) = self.fn_on_response_trailers.clone() {
            self.store.data_mut().effective_context = http_ctx_id;
            if let Some(ctx) = self.store.data_mut().contexts.get_mut(&http_ctx_id) {
                ctx.stage = ContextStage::ResponseTrailers;
                ctx.continue_requested = false;
            }
            let _ = f.call(&mut self.store, (http_ctx_id, 0)).map_err(|e| WasmHostError::GuestTrap {
                hook: "on_response_trailers",
                source: e,
            })?;
            // guest 可能改了 response_headers → 同步回 final_headers
            if let Some(ctx) = self.store.data().contexts.get(&http_ctx_id) {
                final_headers = ctx.response_headers.clone();
            }
        }

        // ─── on_log + on_done + on_delete ───
        self.invoke_log_done_delete(http_ctx_id)?;

        let mut new_resp_parts = resp_parts;
        new_resp_parts.headers = final_headers;
        Ok(SgResponse::from_parts(new_resp_parts, final_body))
    }

    /// 在 guest 返回 Pause 之后，不停地 await dispatch_rx 来驱动状态机，
    /// 直到 guest `continue_stream(HTTP_REQUEST/RESPONSE)` 或写了 `local_response`。
    async fn drive_until_continue(&mut self, ctx_id: u32) -> Result<(), WasmHostError> {
        loop {
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
            let Some((token, result)) = self.dispatch_rx.recv().await else {
                return Err(WasmHostError::Dispatch("dispatch channel closed".to_string()));
            };
            let source_ctx_id = self.store.data_mut().pending_calls.remove(&token).map(|p| p.source_context_id).unwrap_or(ctx_id);
            let header_count;
            let body_len;
            {
                let st = self.store.data_mut();
                st.effective_context = source_ctx_id;
                if let Some(ctx) = st.contexts.get_mut(&source_ctx_id) {
                    ctx.last_call_headers = result.headers.clone();
                    ctx.last_call_status = result.status;
                    ctx.last_call_status_message = result.status_message.clone();
                    ctx.last_call_body = result.body.clone();
                    ctx.continue_requested = false;
                }
                header_count = result.headers.len() as u32 + 1;
                body_len = result.body.len() as u32;
            }
            debug!(target: "spacegate_plugin_wasm", token, source_ctx_id, status = result.status, body_len, "fire proxy_on_http_call_response");
            let f = self.fn_on_http_call_response.clone();
            f.call(&mut self.store, (source_ctx_id, token, header_count, body_len, 0)).map_err(|e| WasmHostError::GuestTrap {
                hook: "on_http_call_response",
                source: e,
            })?;
        }
    }

    fn invoke_log_done_delete(&mut self, ctx_id: u32) -> Result<(), WasmHostError> {
        self.store.data_mut().effective_context = ctx_id;
        if let Some(ctx) = self.store.data_mut().contexts.get_mut(&ctx_id) {
            ctx.stage = ContextStage::Log;
        }
        if let Some(f) = self.fn_on_log.clone() {
            let _ = f.call(&mut self.store, ctx_id);
        }
        if let Some(f) = self.fn_on_done.clone() {
            // spec §proxy_on_done：返回 false 表示 plugin 还要再调 `proxy_done`。
            // 当前 http context 在请求结束时即刻销毁，host 没有"再等一会"的空间：
            // 标记 awaiting_done 让 `proxy_done` 能 Ok 一次，guest 若在 on_log 里立刻 done 则完美；
            // 否则强制完成并 warn。
            if let Some(ctx) = self.store.data_mut().contexts.get_mut(&ctx_id) {
                ctx.awaiting_done = true;
            }
            let v = f.call(&mut self.store, ctx_id).unwrap_or(1);
            let done = v != 0 || self.store.data().contexts.get(&ctx_id).map(|c| c.done_marker).unwrap_or(true);
            if !done {
                warn!(
                    target: "spacegate_plugin_wasm",
                    ctx_id,
                    "proxy_on_done returned false but http context cannot defer; forcing delete"
                );
            }
        }
        if let Some(f) = self.fn_on_delete.clone() {
            let _ = f.call(&mut self.store, ctx_id);
        }
        self.store.data_mut().contexts.remove(&ctx_id);
        Ok(())
    }

    pub fn fail_strategy(&self) -> FailStrategy {
        self.fail_strategy
    }

    /// guest 当前请求的 `proxy_set_tick_period_milliseconds` 值；0 表示尚未配置 / 已停。
    pub fn tick_period_ms(&self) -> u32 {
        self.store.data().tick_period_ms.unwrap_or(0)
    }

    /// 在 root_context 上同步触发一次 `proxy_on_tick`。host 端后台任务调用本方法。
    ///
    /// 失败要么是 guest trap（要么后台任务自停），要么是 guest 没导出 `proxy_on_tick`——后者直接 Ok。
    pub fn tick(&mut self) -> Result<(), WasmHostError> {
        let Some(f) = self.fn_on_tick.clone() else {
            return Ok(());
        };
        self.store.data_mut().effective_context = self.root_id;
        f.call(&mut self.store, self.root_id).map_err(|e| WasmHostError::GuestTrap { hook: "on_tick", source: e })?;
        Ok(())
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

/// spec §Unimplemented WASI functions + §Logging §Clocks §Randomness：完整的 wasi_snapshot_preview1 子集。
///
/// - `random_get`：用 OS RNG（spec §Randomness）。
/// - `clock_time_get`：spec §Clocks，REALTIME 用 SystemTime，MONOTONIC 用 Instant。
/// - `environ_get` / `environ_sizes_get`：spec 明确不暴露 host env，全部 0。
/// - `fd_write`：spec §Logging：fd=1→INFO，fd=2→ERROR；解析 iovec 提取 bytes。
/// - `args_sizes_get` / `args_get`：spec §Unimplemented WASI，固定写 0。
/// - `proc_exit`：spec §Unimplemented WASI，noop。
pub fn register_wasi_stubs(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    use crate::abi::{wasi_errno, wasi_fd};

    linker.func_wrap(
        "wasi_snapshot_preview1",
        "random_get",
        |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return wasi_errno::FAULT,
            };
            let mut buf = vec![0u8; len.max(0) as usize];
            if getrandom::getrandom(&mut buf).is_err() {
                return wasi_errno::FAULT;
            }
            if mem.write_bytes(caller.as_context_mut(), ptr as u32, &buf).is_err() {
                return wasi_errno::FAULT;
            }
            wasi_errno::SUCCESS
        },
    )?;
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "clock_time_get",
        |mut caller: wasmtime::Caller<'_, HostState>, clock_id: i32, _prec: i64, return_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return wasi_errno::FAULT,
            };
            let nanos: u64 = match clock_id {
                0 => std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0),
                1 => {
                    static EPOCH: once_cell::sync::OnceCell<std::time::Instant> = once_cell::sync::OnceCell::new();
                    let epoch = EPOCH.get_or_init(std::time::Instant::now);
                    epoch.elapsed().as_nanos() as u64
                }
                _ => return wasi_errno::NOTSUP,
            };
            if mem.write_u64(caller.as_context_mut(), return_ptr as u32, nanos).is_err() {
                return wasi_errno::FAULT;
            }
            wasi_errno::SUCCESS
        },
    )?;
    linker.func_wrap("wasi_snapshot_preview1", "environ_get", |_c: wasmtime::Caller<'_, HostState>, _a: i32, _b: i32| -> i32 {
        wasi_errno::SUCCESS
    })?;
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "environ_sizes_get",
        |mut caller: wasmtime::Caller<'_, HostState>, count_ptr: i32, buf_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return wasi_errno::FAULT,
            };
            if mem.write_u32(caller.as_context_mut(), count_ptr as u32, 0).is_err() {
                return wasi_errno::FAULT;
            }
            if mem.write_u32(caller.as_context_mut(), buf_ptr as u32, 0).is_err() {
                return wasi_errno::FAULT;
            }
            wasi_errno::SUCCESS
        },
    )?;
    linker.func_wrap("wasi_snapshot_preview1", "args_get", |_c: wasmtime::Caller<'_, HostState>, _a: i32, _b: i32| -> i32 {
        wasi_errno::SUCCESS
    })?;
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "args_sizes_get",
        |mut caller: wasmtime::Caller<'_, HostState>, argc_ptr: i32, buf_size_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return wasi_errno::FAULT,
            };
            if mem.write_u32(caller.as_context_mut(), argc_ptr as u32, 0).is_err() {
                return wasi_errno::FAULT;
            }
            if mem.write_u32(caller.as_context_mut(), buf_size_ptr as u32, 0).is_err() {
                return wasi_errno::FAULT;
            }
            wasi_errno::SUCCESS
        },
    )?;
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_write",
        |mut caller: wasmtime::Caller<'_, HostState>, fd: i32, iovs: i32, iovs_len: i32, nwritten_ptr: i32| -> i32 {
            // spec §Logging：fd=1→INFO，fd=2→ERROR；其它 fd → BADF。
            if fd != wasi_fd::STDOUT && fd != wasi_fd::STDERR {
                return wasi_errno::BADF;
            }
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return wasi_errno::FAULT,
            };
            // iovec[]：每项 (buf_ptr: u32, buf_len: u32)，共 iovs_len 项。
            let mut total: u32 = 0;
            let mut bytes_out: Vec<u8> = Vec::new();
            for i in 0..(iovs_len as u32) {
                let entry_ptr = (iovs as u32) + i * 8;
                let Ok(buf_ptr) = mem.read_u32(caller.as_context(), entry_ptr) else {
                    return wasi_errno::FAULT;
                };
                let Ok(buf_len) = mem.read_u32(caller.as_context(), entry_ptr + 4) else {
                    return wasi_errno::FAULT;
                };
                let Ok(chunk) = mem.read_bytes(caller.as_context(), buf_ptr, buf_len) else {
                    return wasi_errno::FAULT;
                };
                bytes_out.extend_from_slice(&chunk);
                total = total.saturating_add(buf_len);
            }
            let msg = String::from_utf8_lossy(&bytes_out);
            let msg_trimmed = msg.trim_end_matches('\n');
            if fd == wasi_fd::STDOUT {
                tracing::info!(target: "spacegate_plugin_wasm::guest::stdout", "{msg_trimmed}");
            } else {
                tracing::error!(target: "spacegate_plugin_wasm::guest::stderr", "{msg_trimmed}");
            }
            if mem.write_u32(caller.as_context_mut(), nwritten_ptr as u32, total).is_err() {
                return wasi_errno::FAULT;
            }
            wasi_errno::SUCCESS
        },
    )?;
    linker.func_wrap("wasi_snapshot_preview1", "proc_exit", |_c: wasmtime::Caller<'_, HostState>, _code: i32| {})?;
    Ok(())
}
