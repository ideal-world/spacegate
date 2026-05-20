//! 把 proxy-wasm v0.2.1 全部 host fn 注册到 `wasmtime::Linker<HostState>`。
//!
//! 实现策略：
//!
//! - 全部使用 **同步** `func_wrap`（host 端不需要 await）。
//! - `proxy_http_call` 是唯一的"异步"——它**同步**返回 token，把真正的 HTTP 调用 `tokio::spawn`
//!   出去，结果通过 `dispatch_tx` 投递回 Vm 状态机；Vm 主循环 await。
//! - gRPC / 外部函数：进程内不接 gRPC client / FFI 注册表，返回 `Unimplemented` / `NotFound`。
//! - 命名与 proxy-wasm spec 完全一致；参数按 i32（线性内存偏移/长度均为 i32）。

use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use tracing::{debug, info, warn};
use wasmtime::{AsContext, AsContextMut, Caller, Linker};

use crate::abi::{
    decode_pairs, decode_property_path, encode_pairs, host_max_log_level, log_level_to_tracing, BufferType, LogLevel, MapType, MemoryHelper, MetricType, Status, StreamType,
};
use crate::host_state::{HostState, HttpCallResult, LocalResponse};
use crate::shared::{
    metric_define, metric_get, metric_increment, metric_record, queue_dequeue, queue_enqueue, queue_register, queue_resolve, shared_data_get, shared_data_set, MetricOpResult,
    QueueOpResult, SharedDataSetResult,
};

/// 把所有 proxy-wasm v0.2.1 host fn 注册到 linker。
///
/// `dispatch_tx` 用于把异步 HTTP 调用结果发送给 Vm 状态机。
pub fn register_all(linker: &mut Linker<HostState>, dispatch_tx: tokio::sync::mpsc::UnboundedSender<(u32, HttpCallResult)>) -> Result<(), wasmtime::Error> {
    register_log(linker)?;
    register_clock_and_tick(linker)?;
    register_context_control(linker)?;
    register_stream_control(linker)?;
    register_buffer(linker)?;
    register_headers(linker)?;
    register_status_and_local_response(linker)?;
    register_http_call(linker, dispatch_tx)?;
    register_shared_data_and_queue(linker)?;
    register_metrics(linker)?;
    register_property(linker)?;
    register_grpc_unimplemented(linker)?;
    register_foreign_function(linker)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Logging（spec §Logging）
// ─────────────────────────────────────────────────────────

fn register_log(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "proxy_log", |mut caller: Caller<'_, HostState>, level: i32, msg_ptr: i32, msg_size: i32| -> i32 {
        let mem = match MemoryHelper::from_caller(&mut caller) {
            Ok(m) => m,
            Err(_) => return Status::InvalidMemoryAccess.as_i32(),
        };
        let Ok(msg) = mem.read_string_lossy(caller.as_context(), msg_ptr as u32, msg_size as u32) else {
            return Status::InvalidMemoryAccess.as_i32();
        };
        let Some(lvl) = log_level_to_tracing(level) else {
            return Status::BadArgument.as_i32();
        };
        match lvl {
            tracing::Level::TRACE => tracing::trace!(target: "spacegate_plugin_wasm::guest", "{msg}"),
            tracing::Level::DEBUG => tracing::debug!(target: "spacegate_plugin_wasm::guest", "{msg}"),
            tracing::Level::INFO => tracing::info!(target: "spacegate_plugin_wasm::guest", "{msg}"),
            tracing::Level::WARN => tracing::warn!(target: "spacegate_plugin_wasm::guest", "{msg}"),
            tracing::Level::ERROR => tracing::error!(target: "spacegate_plugin_wasm::guest", "{msg}"),
        }
        Status::Ok.as_i32()
    })?;

    linker.func_wrap("env", "proxy_get_log_level", |mut caller: Caller<'_, HostState>, return_ptr: i32| -> i32 {
        let mem = match MemoryHelper::from_caller(&mut caller) {
            Ok(m) => m,
            Err(_) => return Status::InvalidMemoryAccess.as_i32(),
        };
        let lvl: LogLevel = host_max_log_level();
        if mem.write_u32(caller.as_context_mut(), return_ptr as u32, lvl.as_i32() as u32).is_err() {
            return Status::InvalidMemoryAccess.as_i32();
        }
        Status::Ok.as_i32()
    })?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Clocks / Timers / Context control（spec §Clocks §Timers §Context lifecycle）
// ─────────────────────────────────────────────────────────

fn register_clock_and_tick(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "proxy_get_current_time_nanoseconds", |mut caller: Caller<'_, HostState>, return_ptr: i32| -> i32 {
        let mem = match MemoryHelper::from_caller(&mut caller) {
            Ok(m) => m,
            Err(_) => return Status::InvalidMemoryAccess.as_i32(),
        };
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        if mem.write_u64(caller.as_context_mut(), return_ptr as u32, nanos).is_err() {
            return Status::InvalidMemoryAccess.as_i32();
        }
        Status::Ok.as_i32()
    })?;

    linker.func_wrap("env", "proxy_set_tick_period_milliseconds", |mut caller: Caller<'_, HostState>, period: i32| -> i32 {
        caller.data_mut().tick_period_ms = if period > 0 { Some(period as u32) } else { None };
        Status::Ok.as_i32()
    })?;
    Ok(())
}

fn register_context_control(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_set_effective_context(context_id) -> Status
    linker.func_wrap("env", "proxy_set_effective_context", |mut caller: Caller<'_, HostState>, ctx_id: i32| -> i32 {
        let cid = ctx_id as u32;
        let st = caller.data_mut();
        if st.contexts.contains_key(&cid) || cid == st.root_context_id {
            st.effective_context = cid;
            Status::Ok.as_i32()
        } else {
            Status::BadArgument.as_i32()
        }
    })?;

    // proxy_done() -> Status
    //
    // spec §proxy_done：guest 在 `proxy_on_done` 返回 false 之后调本 hostcall 表示「确实做完了」。
    // host 据此结束等待，进入 on_log/on_delete（在 vm.rs 处理）。
    linker.func_wrap("env", "proxy_done", |mut caller: Caller<'_, HostState>| -> i32 {
        let st = caller.data_mut();
        let cid = st.effective_context;
        if let Some(ctx) = st.contexts.get_mut(&cid) {
            if !ctx.awaiting_done {
                return Status::NotFound.as_i32();
            }
            ctx.done_marker = true;
            ctx.awaiting_done = false;
            Status::Ok.as_i32()
        } else {
            Status::NotFound.as_i32()
        }
    })?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Stream control（spec §Common HTTP and TCP stream operations）
// ─────────────────────────────────────────────────────────

fn register_stream_control(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_continue_stream(stream_type) -> Status
    //
    // 我们 host 端仅处理 HTTP_REQUEST/HTTP_RESPONSE 的 continue：把当前 ctx 的
    // continue_requested 置 true，Vm 状态机据此退出 await loop。Downstream/Upstream
    // 我们不接 TCP 层 → 返回 UNIMPLEMENTED（spec 允许）。
    linker.func_wrap("env", "proxy_continue_stream", |mut caller: Caller<'_, HostState>, stream_type: i32| -> i32 {
        let Some(st_kind) = StreamType::from_i32(stream_type) else {
            return Status::BadArgument.as_i32();
        };
        match st_kind {
            StreamType::HttpRequest | StreamType::HttpResponse => {
                let st = caller.data();
                let ctx_id = st.effective_context;
                if let Some(ctx) = caller.data_mut().contexts.get_mut(&ctx_id) {
                    ctx.continue_requested = true;
                }
                Status::Ok.as_i32()
            }
            StreamType::Downstream | StreamType::Upstream => Status::Unimplemented.as_i32(),
        }
    })?;

    // proxy_close_stream(stream_type) -> Status
    linker.func_wrap("env", "proxy_close_stream", |_caller: Caller<'_, HostState>, stream_type: i32| -> i32 {
        match StreamType::from_i32(stream_type) {
            Some(StreamType::HttpRequest) | Some(StreamType::HttpResponse) => Status::Ok.as_i32(),
            Some(StreamType::Downstream) | Some(StreamType::Upstream) => Status::Unimplemented.as_i32(),
            None => Status::BadArgument.as_i32(),
        }
    })?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Buffers（spec §Buffers）
// ─────────────────────────────────────────────────────────

/// 取 buffer 内容（克隆出一份，避免后续借用冲突）。
fn read_buffer(state: &HostState, buf_type: BufferType) -> Option<Vec<u8>> {
    match buf_type {
        BufferType::PluginConfiguration | BufferType::VmConfiguration => Some(state.configuration.clone()),
        BufferType::HttpRequestBody => state.current_context().and_then(|c| c.request_body.as_ref().map(|b| b.to_vec())),
        BufferType::HttpResponseBody => state.current_context().and_then(|c| c.response_body.as_ref().map(|b| b.to_vec())),
        BufferType::HttpCallResponseBody => state.current_context().map(|c| c.last_call_body.to_vec()),
        // 未支持的（TCP / gRPC / FFI args）：buffer 类型本身合法，但当前 host 无数据 → NotFound
        BufferType::DownstreamData | BufferType::UpstreamData | BufferType::GrpcCallMessage | BufferType::ForeignFunctionArguments => None,
    }
}

fn register_buffer(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_get_buffer_bytes(buffer_type, start, max_size, *return_data, *return_size) -> Status
    linker.func_wrap(
        "env",
        "proxy_get_buffer_bytes",
        |mut caller: Caller<'_, HostState>, buffer_type: i32, start: i32, max_size: i32, return_data_ptr: i32, return_size_ptr: i32| -> i32 {
            let Some(buf_type) = BufferType::from_i32(buffer_type) else {
                return Status::BadArgument.as_i32();
            };
            let bytes_opt = read_buffer(caller.data(), buf_type);
            let Some(bytes) = bytes_opt else {
                return Status::NotFound.as_i32();
            };
            let start = (start as u32) as usize;
            let max_size = (max_size as u32) as usize;
            if start > bytes.len() {
                return Status::BadArgument.as_i32();
            }
            let end = (start.saturating_add(max_size)).min(bytes.len());
            let slice = &bytes[start..end];
            match write_alloc_pair(&mut caller, slice, return_data_ptr as u32, return_size_ptr as u32) {
                Ok(()) => Status::Ok.as_i32(),
                Err(s) => s.as_i32(),
            }
        },
    )?;

    // proxy_get_buffer_status(buffer_type, *return_buffer_size, *return_unused) -> Status
    linker.func_wrap(
        "env",
        "proxy_get_buffer_status",
        |mut caller: Caller<'_, HostState>, buffer_type: i32, return_size_ptr: i32, return_unused_ptr: i32| -> i32 {
            let Some(buf_type) = BufferType::from_i32(buffer_type) else {
                return Status::BadArgument.as_i32();
            };
            let len = match read_buffer(caller.data(), buf_type) {
                Some(b) => b.len() as u32,
                None => return Status::NotFound.as_i32(),
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            if mem.write_u32(caller.as_context_mut(), return_size_ptr as u32, len).is_err() {
                return Status::InvalidMemoryAccess.as_i32();
            }
            let _ = mem.write_u32(caller.as_context_mut(), return_unused_ptr as u32, 0);
            Status::Ok.as_i32()
        },
    )?;

    // proxy_set_buffer_bytes(buffer_type, start, size, *data, data_size) -> Status
    //
    // spec §Buffers proxy_set_buffer_bytes：可做 prepend / append / inject / replace。
    // start, size 解释为：用 (data, data_size) 替换 [start, start+size) 范围。
    linker.func_wrap(
        "env",
        "proxy_set_buffer_bytes",
        |mut caller: Caller<'_, HostState>, buffer_type: i32, start: i32, size: i32, data_ptr: i32, data_size: i32| -> i32 {
            let Some(buf_type) = BufferType::from_i32(buffer_type) else {
                return Status::BadArgument.as_i32();
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let new_bytes = match mem.read_bytes(caller.as_context(), data_ptr as u32, data_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let ctx_id = caller.data().effective_context;
            let st = caller.data_mut();
            let Some(ctx) = st.contexts.get_mut(&ctx_id) else {
                return Status::NotFound.as_i32();
            };
            match buf_type {
                BufferType::HttpRequestBody => {
                    let cur = ctx.request_body.take().unwrap_or_default();
                    ctx.request_body = Some(splice_buffer(&cur, start as u32, size as u32, &new_bytes));
                    Status::Ok.as_i32()
                }
                BufferType::HttpResponseBody => {
                    let cur = ctx.response_body.take().unwrap_or_default();
                    ctx.response_body = Some(splice_buffer(&cur, start as u32, size as u32, &new_bytes));
                    Status::Ok.as_i32()
                }
                // TCP / gRPC / 配置 / FFI args：本 host 不支持写
                BufferType::DownstreamData
                | BufferType::UpstreamData
                | BufferType::GrpcCallMessage
                | BufferType::VmConfiguration
                | BufferType::PluginConfiguration
                | BufferType::HttpCallResponseBody
                | BufferType::ForeignFunctionArguments => Status::BadArgument.as_i32(),
            }
        },
    )?;

    Ok(())
}

/// spec §proxy_set_buffer_bytes：用 `replacement` 替换 `cur[start..start+size]`。
fn splice_buffer(cur: &Bytes, start: u32, size: u32, replacement: &[u8]) -> Bytes {
    let cur_len = cur.len();
    let start = (start as usize).min(cur_len);
    let size = (size as usize).min(cur_len.saturating_sub(start));
    let mut out = Vec::with_capacity(cur_len.saturating_add(replacement.len()));
    out.extend_from_slice(&cur[..start]);
    out.extend_from_slice(replacement);
    out.extend_from_slice(&cur[start + size..]);
    Bytes::from(out)
}

// ─────────────────────────────────────────────────────────
// HTTP fields（spec §HTTP fields）
// ─────────────────────────────────────────────────────────

fn register_headers(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_get_header_map_size(map_type, *return_size) -> Status
    linker.func_wrap(
        "env",
        "proxy_get_header_map_size",
        |mut caller: Caller<'_, HostState>, map_type: i32, return_size_ptr: i32| -> i32 {
            let Some(mt) = MapType::from_i32(map_type) else {
                return Status::BadArgument.as_i32();
            };
            let pairs = collect_pairs(caller.data(), mt);
            let buf = {
                let refs: Vec<(&[u8], &[u8])> = pairs.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();
                encode_pairs(&refs)
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            if mem.write_u32(caller.as_context_mut(), return_size_ptr as u32, buf.len() as u32).is_err() {
                return Status::InvalidMemoryAccess.as_i32();
            }
            Status::Ok.as_i32()
        },
    )?;

    // proxy_get_header_map_pairs
    linker.func_wrap(
        "env",
        "proxy_get_header_map_pairs",
        |mut caller: Caller<'_, HostState>, map_type: i32, return_data_ptr: i32, return_size_ptr: i32| -> i32 {
            let Some(mt) = MapType::from_i32(map_type) else {
                return Status::BadArgument.as_i32();
            };
            let pairs = collect_pairs(caller.data(), mt);
            let buf = {
                let refs: Vec<(&[u8], &[u8])> = pairs.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();
                encode_pairs(&refs)
            };
            match write_alloc_pair(&mut caller, &buf, return_data_ptr as u32, return_size_ptr as u32) {
                Ok(()) => Status::Ok.as_i32(),
                Err(s) => s.as_i32(),
            }
        },
    )?;

    // proxy_set_header_map_pairs
    linker.func_wrap(
        "env",
        "proxy_set_header_map_pairs",
        |mut caller: Caller<'_, HostState>, map_type: i32, data_ptr: i32, data_size: i32| -> i32 {
            let Some(mt) = MapType::from_i32(map_type) else {
                return Status::BadArgument.as_i32();
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let raw = match mem.read_bytes(caller.as_context(), data_ptr as u32, data_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let Some(pairs) = decode_pairs(&raw) else {
                return Status::SerializationFailure.as_i32();
            };
            let new_map = pairs_to_header_map(&pairs);
            replace_map(caller.data_mut(), mt, new_map);
            Status::Ok.as_i32()
        },
    )?;

    // proxy_get_header_map_value
    linker.func_wrap(
        "env",
        "proxy_get_header_map_value",
        |mut caller: Caller<'_, HostState>, map_type: i32, key_ptr: i32, key_size: i32, return_data_ptr: i32, return_size_ptr: i32| -> i32 {
            let Some(mt) = MapType::from_i32(map_type) else {
                return Status::BadArgument.as_i32();
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let key = match mem.read_string_lossy(caller.as_context(), key_ptr as u32, key_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let key_l = key.to_ascii_lowercase();
            let Some(value) = lookup_header(caller.data(), mt, &key_l) else {
                return Status::NotFound.as_i32();
            };
            let bytes = value.into_bytes();
            match write_alloc_pair(&mut caller, &bytes, return_data_ptr as u32, return_size_ptr as u32) {
                Ok(()) => Status::Ok.as_i32(),
                Err(s) => s.as_i32(),
            }
        },
    )?;

    // proxy_add_header_map_value
    linker.func_wrap(
        "env",
        "proxy_add_header_map_value",
        |mut caller: Caller<'_, HostState>, map_type: i32, key_ptr: i32, key_size: i32, value_ptr: i32, value_size: i32| -> i32 {
            let Some(mt) = MapType::from_i32(map_type) else {
                return Status::BadArgument.as_i32();
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let key = match mem.read_string_lossy(caller.as_context(), key_ptr as u32, key_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let value = match mem.read_string_lossy(caller.as_context(), value_ptr as u32, value_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            mutate_header(caller.data_mut(), mt, &key, HeaderMutation::Add(value))
        },
    )?;

    // proxy_replace_header_map_value
    linker.func_wrap(
        "env",
        "proxy_replace_header_map_value",
        |mut caller: Caller<'_, HostState>, map_type: i32, key_ptr: i32, key_size: i32, value_ptr: i32, value_size: i32| -> i32 {
            let Some(mt) = MapType::from_i32(map_type) else {
                return Status::BadArgument.as_i32();
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let key = match mem.read_string_lossy(caller.as_context(), key_ptr as u32, key_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let value = match mem.read_string_lossy(caller.as_context(), value_ptr as u32, value_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            mutate_header(caller.data_mut(), mt, &key, HeaderMutation::Replace(value))
        },
    )?;

    // proxy_remove_header_map_value
    linker.func_wrap(
        "env",
        "proxy_remove_header_map_value",
        |mut caller: Caller<'_, HostState>, map_type: i32, key_ptr: i32, key_size: i32| -> i32 {
            let Some(mt) = MapType::from_i32(map_type) else {
                return Status::BadArgument.as_i32();
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let key = match mem.read_string_lossy(caller.as_context(), key_ptr as u32, key_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            mutate_header(caller.data_mut(), mt, &key, HeaderMutation::Remove)
        },
    )?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Local response / status（spec §HTTP streams §proxy_send_local_response）
// ─────────────────────────────────────────────────────────

fn register_status_and_local_response(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_get_status(*return_status_code, **msg_data, *msg_size) -> Status
    //
    // spec §proxy_get_status：在 on_http_call_response 中返回该次 HTTP 调用的 status；
    // 其它时机我们返回当前响应 status。
    linker.func_wrap(
        "env",
        "proxy_get_status",
        |mut caller: Caller<'_, HostState>, status_code_ptr: i32, msg_data_ptr: i32, msg_size_ptr: i32| -> i32 {
            let (code, msg): (u32, String) = match caller.data().current_context() {
                Some(c) => {
                    if c.last_call_status > 0 {
                        (c.last_call_status as u32, c.last_call_status_message.clone())
                    } else if let Some(rs) = c.response_status {
                        (rs as u32, c.response_status_message.clone())
                    } else {
                        (0, String::new())
                    }
                }
                None => (0, String::new()),
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            if mem.write_u32(caller.as_context_mut(), status_code_ptr as u32, code).is_err() {
                return Status::InvalidMemoryAccess.as_i32();
            }
            let bytes = msg.into_bytes();
            match write_alloc_pair(&mut caller, &bytes, msg_data_ptr as u32, msg_size_ptr as u32) {
                Ok(()) => Status::Ok.as_i32(),
                Err(s) => s.as_i32(),
            }
        },
    )?;

    // proxy_send_local_response(status, *status_text, status_text_size, *body, body_size, *headers, headers_size, grpc_status)
    linker.func_wrap(
        "env",
        "proxy_send_local_response",
        |mut caller: Caller<'_, HostState>,
         status: i32,
         _status_text_data: i32,
         _status_text_size: i32,
         body_data: i32,
         body_size: i32,
         headers_data: i32,
         headers_size: i32,
         _grpc_status: i32|
         -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let body = if body_size > 0 {
                match mem.read_bytes(caller.as_context(), body_data as u32, body_size as u32) {
                    Ok(b) => b,
                    Err(_) => return Status::InvalidMemoryAccess.as_i32(),
                }
            } else {
                Vec::new()
            };
            let headers_bytes = if headers_size > 0 {
                match mem.read_bytes(caller.as_context(), headers_data as u32, headers_size as u32) {
                    Ok(b) => b,
                    Err(_) => return Status::InvalidMemoryAccess.as_i32(),
                }
            } else {
                Vec::new()
            };
            let pairs = decode_pairs(&headers_bytes).unwrap_or_default();
            let map = pairs_to_header_map(&pairs);
            let ctx_id = caller.data().effective_context;
            if let Some(ctx) = caller.data_mut().contexts.get_mut(&ctx_id) {
                ctx.local_response = Some(LocalResponse {
                    status: status as u16,
                    headers: map,
                    body: Bytes::from(body),
                });
                debug!(target: "spacegate_plugin_wasm", ctx_id, status, "guest send_local_response captured");
            } else {
                warn!(target: "spacegate_plugin_wasm", ctx_id, "send_local_response on unknown ctx");
            }
            Status::Ok.as_i32()
        },
    )?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// proxy_http_call（spec §HTTP calls）
// ─────────────────────────────────────────────────────────

fn register_http_call(linker: &mut Linker<HostState>, dispatch_tx: tokio::sync::mpsc::UnboundedSender<(u32, HttpCallResult)>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "env",
        "proxy_http_call",
        move |mut caller: Caller<'_, HostState>,
              upstream_data: i32,
              upstream_size: i32,
              headers_data: i32,
              headers_size: i32,
              body_data: i32,
              body_size: i32,
              _trailers_data: i32,
              _trailers_size: i32,
              timeout_ms: i32,
              return_token_ptr: i32|
              -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let cluster = match mem.read_string_lossy(caller.as_context(), upstream_data as u32, upstream_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let headers_bytes = mem.read_bytes(caller.as_context(), headers_data as u32, headers_size as u32).unwrap_or_default();
            let body = if body_size > 0 {
                if let Some(limit) = caller.data().shell_cfg.limits.max_body_bytes {
                    if body_size as usize > limit {
                        warn!(target: "spacegate_plugin_wasm", body_size, limit, "dispatch_http_call: request body exceeds max_body_bytes");
                        return Status::BadArgument.as_i32();
                    }
                }
                mem.read_bytes(caller.as_context(), body_data as u32, body_size as u32).unwrap_or_default()
            } else {
                Vec::new()
            };
            let pairs = match decode_pairs(&headers_bytes) {
                Some(p) => p,
                None => return Status::SerializationFailure.as_i32(),
            };
            let mut method = "GET".to_string();
            let mut path = "/".to_string();
            let mut authority = String::new();
            let mut others = Vec::with_capacity(pairs.len());
            for (k, v) in &pairs {
                let key_str = String::from_utf8_lossy(k);
                let val_str = String::from_utf8_lossy(v).into_owned();
                match key_str.as_ref() {
                    ":method" => method = val_str,
                    ":path" => path = val_str,
                    ":authority" => authority = val_str,
                    ":scheme" => {}
                    _ => others.push((key_str.to_string(), val_str)),
                }
            }
            if method.is_empty() || path.is_empty() {
                return Status::BadArgument.as_i32();
            }
            let st = caller.data();
            let base = st.shell_cfg.resolve_cluster(&cluster).or_else(|| if !authority.is_empty() { Some(format!("http://{authority}")) } else { None });
            let Some(base) = base else {
                warn!(target: "spacegate_plugin_wasm", cluster = %cluster, "dispatch_http_call: cluster not configured");
                return Status::BadArgument.as_i32();
            };
            if let Some(limit) = caller.data().shell_cfg.limits.max_pending_calls {
                if caller.data().pending_calls.len() >= limit {
                    warn!(
                        target: "spacegate_plugin_wasm",
                        pending_calls = caller.data().pending_calls.len(),
                        limit,
                        "dispatch_http_call: max_pending_calls reached"
                    );
                    return Status::InternalFailure.as_i32();
                }
            }
            let url = format!("{}{}", base.trim_end_matches('/'), path);
            let token = caller.data_mut().next_dispatch_token();
            let source_ctx = caller.data().effective_context;
            caller.data_mut().pending_calls.insert(
                token,
                crate::host_state::PendingCall {
                    waker: None,
                    source_context_id: source_ctx,
                },
            );
            let client = caller.data().http_client.clone();
            let max_body_bytes = caller.data().shell_cfg.limits.max_body_bytes;
            let timeout = Duration::from_millis(timeout_ms.max(1) as u64);
            let tx = dispatch_tx.clone();
            tokio::spawn(async move {
                debug!(target: "spacegate_plugin_wasm", %url, %method, "dispatch_http_call begin");
                let parsed_method = method.parse::<reqwest::Method>().unwrap_or(reqwest::Method::GET);
                let mut req = client.request(parsed_method, &url);
                for (k, v) in others {
                    if k.starts_with(':') {
                        continue;
                    }
                    if let (Ok(name), Ok(val)) = (HeaderName::try_from(k.as_str()), HeaderValue::try_from(v.as_str())) {
                        req = req.header(name, val);
                    }
                }
                if !body.is_empty() {
                    req = req.body(body);
                }
                req = req.timeout(timeout);
                let result = match req.send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        let status_message = resp.status().canonical_reason().unwrap_or("").to_string();
                        let mut hdrs = HeaderMap::new();
                        for (k, v) in resp.headers().iter() {
                            if let (Ok(name), Ok(val)) = (HeaderName::try_from(k.as_str()), HeaderValue::from_bytes(v.as_bytes())) {
                                hdrs.append(name, val);
                            }
                        }
                        let body_bytes = resp.bytes().await.unwrap_or_default();
                        if let Some(limit) = max_body_bytes {
                            if body_bytes.len() > limit {
                                warn!(
                                    target: "spacegate_plugin_wasm",
                                    %url,
                                    body_len = body_bytes.len(),
                                    limit,
                                    "dispatch_http_call response exceeds max_body_bytes"
                                );
                                HttpCallResult {
                                    status: 0,
                                    status_message: format!("dispatch_http_call response body too large: {} > {limit}", body_bytes.len()),
                                    headers: HeaderMap::new(),
                                    body: Bytes::new(),
                                }
                            } else {
                                HttpCallResult {
                                    status,
                                    status_message,
                                    headers: hdrs,
                                    body: body_bytes,
                                }
                            }
                        } else {
                            HttpCallResult {
                                status,
                                status_message,
                                headers: hdrs,
                                body: body_bytes,
                            }
                        }
                    }
                    Err(e) => {
                        warn!(target: "spacegate_plugin_wasm", %url, error = %e, "dispatch_http_call failed");
                        HttpCallResult {
                            status: 0,
                            status_message: format!("{e}"),
                            headers: HeaderMap::new(),
                            body: Bytes::new(),
                        }
                    }
                };
                debug!(target: "spacegate_plugin_wasm", token, status = result.status, "dispatch_http_call done");
                let _ = tx.send((token, result));
            });
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            if mem.write_u32(caller.as_context_mut(), return_token_ptr as u32, token).is_err() {
                return Status::InvalidMemoryAccess.as_i32();
            }
            info!(target: "spacegate_plugin_wasm", token, cluster = %cluster, "dispatch_http_call enqueued");
            Status::Ok.as_i32()
        },
    )?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Shared Data / Shared Queues（spec §Shared Key-Value Store §Shared Queues）
// ─────────────────────────────────────────────────────────

fn register_shared_data_and_queue(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_get_shared_data(*k, k_size, **v, *v_size, *cas) -> Status
    linker.func_wrap(
        "env",
        "proxy_get_shared_data",
        |mut caller: Caller<'_, HostState>, k_ptr: i32, k_size: i32, v_data_ptr: i32, v_size_ptr: i32, cas_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let key = match mem.read_bytes(caller.as_context(), k_ptr as u32, k_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let Some((value, cas)) = shared_data_get(&key) else {
                return Status::NotFound.as_i32();
            };
            if let Err(s) = write_alloc_pair(&mut caller, &value, v_data_ptr as u32, v_size_ptr as u32) {
                return s.as_i32();
            }
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            if mem.write_u32(caller.as_context_mut(), cas_ptr as u32, cas).is_err() {
                return Status::InvalidMemoryAccess.as_i32();
            }
            Status::Ok.as_i32()
        },
    )?;

    // proxy_set_shared_data(*k, k_size, *v, v_size, cas) -> Status
    linker.func_wrap(
        "env",
        "proxy_set_shared_data",
        |mut caller: Caller<'_, HostState>, k_ptr: i32, k_size: i32, v_ptr: i32, v_size: i32, cas: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let key = match mem.read_bytes(caller.as_context(), k_ptr as u32, k_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let value = if v_size > 0 {
                match mem.read_bytes(caller.as_context(), v_ptr as u32, v_size as u32) {
                    Ok(b) => b,
                    Err(_) => return Status::InvalidMemoryAccess.as_i32(),
                }
            } else {
                Vec::new()
            };
            match shared_data_set(&key, &value, cas as u32) {
                SharedDataSetResult::Ok => Status::Ok.as_i32(),
                SharedDataSetResult::CasMismatch => Status::CasMismatch.as_i32(),
            }
        },
    )?;

    // proxy_register_shared_queue(*n, n_size, *return_qid) -> Status
    linker.func_wrap(
        "env",
        "proxy_register_shared_queue",
        |mut caller: Caller<'_, HostState>, n_ptr: i32, n_size: i32, return_qid_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let name = match mem.read_string_lossy(caller.as_context(), n_ptr as u32, n_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let vm_id = caller.data().plugin_vm_id.clone();
            let qid = queue_register(&vm_id, &name);
            if mem.write_u32(caller.as_context_mut(), return_qid_ptr as u32, qid).is_err() {
                return Status::InvalidMemoryAccess.as_i32();
            }
            Status::Ok.as_i32()
        },
    )?;

    // proxy_resolve_shared_queue(*vm_id, vm_id_size, *n, n_size, *return_qid) -> Status
    linker.func_wrap(
        "env",
        "proxy_resolve_shared_queue",
        |mut caller: Caller<'_, HostState>, vid_ptr: i32, vid_size: i32, n_ptr: i32, n_size: i32, return_qid_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let vid = match mem.read_string_lossy(caller.as_context(), vid_ptr as u32, vid_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let name = match mem.read_string_lossy(caller.as_context(), n_ptr as u32, n_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let Some(qid) = queue_resolve(&vid, &name) else {
                return Status::NotFound.as_i32();
            };
            if mem.write_u32(caller.as_context_mut(), return_qid_ptr as u32, qid).is_err() {
                return Status::InvalidMemoryAccess.as_i32();
            }
            Status::Ok.as_i32()
        },
    )?;

    // proxy_enqueue_shared_queue(qid, *v, v_size) -> Status
    linker.func_wrap(
        "env",
        "proxy_enqueue_shared_queue",
        |mut caller: Caller<'_, HostState>, qid: i32, v_ptr: i32, v_size: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let bytes = if v_size > 0 {
                match mem.read_bytes(caller.as_context(), v_ptr as u32, v_size as u32) {
                    Ok(b) => b,
                    Err(_) => return Status::InvalidMemoryAccess.as_i32(),
                }
            } else {
                Vec::new()
            };
            match queue_enqueue(qid as u32, &bytes) {
                QueueOpResult::Ok => Status::Ok.as_i32(),
                QueueOpResult::NotFound => Status::NotFound.as_i32(),
                QueueOpResult::Empty => Status::Empty.as_i32(),
            }
        },
    )?;

    // proxy_dequeue_shared_queue(qid, **v, *v_size) -> Status
    linker.func_wrap(
        "env",
        "proxy_dequeue_shared_queue",
        |mut caller: Caller<'_, HostState>, qid: i32, v_data_ptr: i32, v_size_ptr: i32| -> i32 {
            match queue_dequeue(qid as u32) {
                (QueueOpResult::Ok, Some(bytes)) => match write_alloc_pair(&mut caller, &bytes, v_data_ptr as u32, v_size_ptr as u32) {
                    Ok(()) => Status::Ok.as_i32(),
                    Err(s) => s.as_i32(),
                },
                (QueueOpResult::NotFound, _) => Status::NotFound.as_i32(),
                _ => Status::Empty.as_i32(),
            }
        },
    )?;

    Ok(())
}

// ─────────────────────────────────────────────────────────
// Metrics（spec §Metrics）
// ─────────────────────────────────────────────────────────

fn register_metrics(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_define_metric(metric_type, *name, name_size, *return_mid) -> Status
    linker.func_wrap(
        "env",
        "proxy_define_metric",
        |mut caller: Caller<'_, HostState>, metric_type: i32, name_ptr: i32, name_size: i32, return_mid_ptr: i32| -> i32 {
            let Some(kind) = MetricType::from_i32(metric_type) else {
                return Status::BadArgument.as_i32();
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let name = match mem.read_string_lossy(caller.as_context(), name_ptr as u32, name_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let id = metric_define(kind, &name);
            if mem.write_u32(caller.as_context_mut(), return_mid_ptr as u32, id).is_err() {
                return Status::InvalidMemoryAccess.as_i32();
            }
            Status::Ok.as_i32()
        },
    )?;

    // proxy_record_metric(mid, value: u64) -> Status
    linker.func_wrap("env", "proxy_record_metric", |_caller: Caller<'_, HostState>, mid: i32, value: i64| -> i32 {
        match metric_record(mid as u32, value as u64) {
            MetricOpResult::Ok => Status::Ok.as_i32(),
            MetricOpResult::NotFound => Status::NotFound.as_i32(),
            MetricOpResult::BadArgument => Status::BadArgument.as_i32(),
        }
    })?;

    // proxy_increment_metric(mid, delta: i64) -> Status
    linker.func_wrap("env", "proxy_increment_metric", |_caller: Caller<'_, HostState>, mid: i32, delta: i64| -> i32 {
        match metric_increment(mid as u32, delta) {
            MetricOpResult::Ok => Status::Ok.as_i32(),
            MetricOpResult::NotFound => Status::NotFound.as_i32(),
            MetricOpResult::BadArgument => Status::BadArgument.as_i32(),
        }
    })?;

    // proxy_get_metric(mid, *return_value) -> Status
    linker.func_wrap("env", "proxy_get_metric", |mut caller: Caller<'_, HostState>, mid: i32, return_ptr: i32| -> i32 {
        let Some(v) = metric_get(mid as u32) else {
            return Status::NotFound.as_i32();
        };
        let mem = match MemoryHelper::from_caller(&mut caller) {
            Ok(m) => m,
            Err(_) => return Status::InvalidMemoryAccess.as_i32(),
        };
        if mem.write_u64(caller.as_context_mut(), return_ptr as u32, v).is_err() {
            return Status::InvalidMemoryAccess.as_i32();
        }
        Status::Ok.as_i32()
    })?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Properties（spec §Properties）
// ─────────────────────────────────────────────────────────

fn register_property(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_get_property(*path, path_size, **v, *v_size) -> Status
    linker.func_wrap(
        "env",
        "proxy_get_property",
        |mut caller: Caller<'_, HostState>, path_ptr: i32, path_size: i32, return_data_ptr: i32, return_size_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let raw = match mem.read_bytes(caller.as_context(), path_ptr as u32, path_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let segments = decode_property_path(&raw);
            if segments.is_empty() {
                return Status::NotFound.as_i32();
            }
            // 1. 用户通过 proxy_set_property 写入的优先（spec 允许 host 自行决定）
            let canonical_key = canonicalize_path(&segments);
            if let Some(v) = caller.data().user_properties.get(&canonical_key).cloned() {
                return match write_alloc_pair(&mut caller, &v, return_data_ptr as u32, return_size_ptr as u32) {
                    Ok(()) => Status::Ok.as_i32(),
                    Err(s) => s.as_i32(),
                };
            }
            // 2. well-known
            let value = resolve_well_known(caller.data(), &segments);
            let Some(value) = value else {
                return Status::NotFound.as_i32();
            };
            match write_alloc_pair(&mut caller, &value, return_data_ptr as u32, return_size_ptr as u32) {
                Ok(()) => Status::Ok.as_i32(),
                Err(s) => s.as_i32(),
            }
        },
    )?;

    // proxy_set_property(*path, path_size, *v, v_size) -> Status
    linker.func_wrap(
        "env",
        "proxy_set_property",
        |mut caller: Caller<'_, HostState>, path_ptr: i32, path_size: i32, v_ptr: i32, v_size: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let raw_path = match mem.read_bytes(caller.as_context(), path_ptr as u32, path_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::InvalidMemoryAccess.as_i32(),
            };
            let segments = decode_property_path(&raw_path);
            if segments.is_empty() {
                return Status::BadArgument.as_i32();
            }
            let canonical_key = canonicalize_path(&segments);
            let value = if v_size > 0 {
                match mem.read_bytes(caller.as_context(), v_ptr as u32, v_size as u32) {
                    Ok(b) => b,
                    Err(_) => return Status::InvalidMemoryAccess.as_i32(),
                }
            } else {
                Vec::new()
            };
            caller.data_mut().user_properties.insert(canonical_key, value);
            Status::Ok.as_i32()
        },
    )?;
    Ok(())
}

fn canonicalize_path(segments: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        if i > 0 {
            out.push(0);
        }
        out.extend_from_slice(s);
    }
    out
}

/// spec §Properties §Well-known properties：内置覆盖最常用的几个，其它返回 None。
fn resolve_well_known(state: &HostState, segments: &[&[u8]]) -> Option<Vec<u8>> {
    let path_str: Vec<&str> = segments.iter().filter_map(|s| std::str::from_utf8(s).ok()).collect();
    let joined = path_str.join(".");
    match joined.as_str() {
        // Proxy-Wasm
        "plugin_name" => Some(state.plugin_name.as_bytes().to_vec()),
        "plugin_root_id" => Some(state.plugin_root_id.as_bytes().to_vec()),
        "plugin_vm_id" => Some(state.plugin_vm_id.as_bytes().to_vec()),
        // Downstream connection
        "source.address" => state.source_addr.map(|s| s.to_string().into_bytes()).or_else(|| {
            // 退路：从 :authority 推导
            state.current_context().map(|c| c.request_pseudo.authority.clone().into_bytes()).filter(|b| !b.is_empty())
        }),
        "source.port" => state.source_addr.map(|s| s.port().to_string().into_bytes()),
        "destination.address" => state.destination_addr.map(|s| s.to_string().into_bytes()),
        "destination.port" => state.destination_addr.map(|s| s.port().to_string().into_bytes()),
        // HTTP request
        "request.protocol" => state.current_context().map(|c| c.request_protocol.as_bytes().to_vec()).filter(|b| !b.is_empty()),
        "request.size" => state.current_context().map(|c| c.request_size.to_string().into_bytes()),
        "request.total_size" => state.current_context().map(|c| {
            let hdr_bytes = approx_header_bytes(&c.request_headers);
            (c.request_size + hdr_bytes as u64).to_string().into_bytes()
        }),
        // HTTP response
        "response.size" => state.current_context().map(|c| c.response_size.to_string().into_bytes()),
        "response.total_size" => state.current_context().map(|c| {
            let hdr_bytes = approx_header_bytes(&c.response_headers);
            (c.response_size + hdr_bytes as u64).to_string().into_bytes()
        }),
        _ => None,
    }
}

fn approx_header_bytes(map: &HeaderMap) -> usize {
    let mut sum = 0;
    for (k, v) in map.iter() {
        sum += k.as_str().len() + 2 + v.as_bytes().len() + 2;
    }
    sum
}

// ─────────────────────────────────────────────────────────
// gRPC（spec §gRPC calls）→ 全部返回 UNIMPLEMENTED
// ─────────────────────────────────────────────────────────

fn register_grpc_unimplemented(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "env",
        "proxy_grpc_call",
        |_caller: Caller<'_, HostState>, _a: i32, _b: i32, _c: i32, _d: i32, _e: i32, _f: i32, _g: i32, _h: i32, _i: i32, _j: i32, _k: i32, _l: i32| -> i32 {
            Status::Unimplemented.as_i32()
        },
    )?;
    linker.func_wrap(
        "env",
        "proxy_grpc_stream",
        |_caller: Caller<'_, HostState>, _a: i32, _b: i32, _c: i32, _d: i32, _e: i32, _f: i32, _g: i32, _h: i32, _i: i32| -> i32 { Status::Unimplemented.as_i32() },
    )?;
    linker.func_wrap("env", "proxy_grpc_cancel", |_caller: Caller<'_, HostState>, _t: i32| -> i32 {
        Status::Unimplemented.as_i32()
    })?;
    linker.func_wrap("env", "proxy_grpc_close", |_caller: Caller<'_, HostState>, _t: i32| -> i32 {
        Status::Unimplemented.as_i32()
    })?;
    linker.func_wrap("env", "proxy_grpc_send", |_caller: Caller<'_, HostState>, _t: i32, _m: i32, _ms: i32, _eos: i32| -> i32 {
        Status::Unimplemented.as_i32()
    })?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Foreign function（spec §FFI）→ 没有注册表 → NotFound
// ─────────────────────────────────────────────────────────

fn register_foreign_function(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "env",
        "proxy_call_foreign_function",
        |_caller: Caller<'_, HostState>, _a: i32, _b: i32, _c: i32, _d: i32, _e: i32, _f: i32| -> i32 { Status::NotFound.as_i32() },
    )?;
    Ok(())
}

// ─────────────────────────────────────────────────────────
// 辅助：alloc + 写 (data, size) pair；lookup / mutate / collect
// ─────────────────────────────────────────────────────────

/// 在 guest 侧分配一段内存、写入 `payload`、然后把 (guest_ptr, len) 回写到
/// `return_data_ptr` / `return_size_ptr`。
///
/// 空 payload：写 (0, 0)。
fn write_alloc_pair(caller: &mut Caller<'_, HostState>, payload: &[u8], return_data_ptr: u32, return_size_ptr: u32) -> Result<(), Status> {
    let mem = MemoryHelper::from_caller(caller).map_err(|_| Status::InvalidMemoryAccess)?;
    if payload.is_empty() {
        mem.write_u32(caller.as_context_mut(), return_data_ptr, 0).map_err(|_| Status::InvalidMemoryAccess)?;
        mem.write_u32(caller.as_context_mut(), return_size_ptr, 0).map_err(|_| Status::InvalidMemoryAccess)?;
        return Ok(());
    }
    let alloc = caller.data().alloc.clone().ok_or(Status::InternalFailure)?;
    let guest_ptr = alloc.call(&mut *caller, payload.len() as u32).map_err(|_| Status::InternalFailure)?;
    let mem = MemoryHelper::from_caller(caller).map_err(|_| Status::InvalidMemoryAccess)?;
    mem.write_bytes(caller.as_context_mut(), guest_ptr, payload).map_err(|_| Status::InvalidMemoryAccess)?;
    mem.write_u32(caller.as_context_mut(), return_data_ptr, guest_ptr).map_err(|_| Status::InvalidMemoryAccess)?;
    mem.write_u32(caller.as_context_mut(), return_size_ptr, payload.len() as u32).map_err(|_| Status::InvalidMemoryAccess)?;
    Ok(())
}

fn lookup_header(state: &HostState, mt: MapType, key_lower: &str) -> Option<String> {
    let ctx = state.current_context()?;
    let map = match mt {
        MapType::HttpRequestHeaders => &ctx.request_headers,
        MapType::HttpRequestTrailers => &ctx.request_trailers,
        MapType::HttpResponseHeaders => &ctx.response_headers,
        MapType::HttpResponseTrailers => &ctx.response_trailers,
        MapType::HttpCallResponseHeaders => &ctx.last_call_headers,
        MapType::HttpCallResponseTrailers => &ctx.last_call_trailers,
        MapType::GrpcCallInitialMetadata | MapType::GrpcCallTrailingMetadata => return None,
    };
    if let Some(value) = pseudo_lookup(ctx, mt, key_lower) {
        return Some(value);
    }
    let name = HeaderName::try_from(key_lower).ok()?;
    let val = map.get(&name)?;
    val.to_str().ok().map(|s| s.to_string())
}

fn pseudo_lookup(ctx: &crate::host_state::RequestContext, mt: MapType, key: &str) -> Option<String> {
    match (mt, key) {
        (MapType::HttpRequestHeaders, ":method") => Some(ctx.request_pseudo.method.clone()),
        (MapType::HttpRequestHeaders, ":path") => Some(ctx.request_pseudo.path.clone()),
        (MapType::HttpRequestHeaders, ":authority") => Some(ctx.request_pseudo.authority.clone()),
        (MapType::HttpRequestHeaders, ":scheme") => Some(ctx.request_pseudo.scheme.clone()),
        (MapType::HttpResponseHeaders, ":status") => ctx.response_status.map(|s| s.to_string()),
        (MapType::HttpCallResponseHeaders, ":status") => {
            if ctx.last_call_status > 0 {
                Some(ctx.last_call_status.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

enum HeaderMutation {
    Add(String),
    Replace(String),
    Remove,
}

fn mutate_header(state: &mut HostState, mt: MapType, key: &str, m: HeaderMutation) -> i32 {
    if matches!(mt, MapType::GrpcCallInitialMetadata | MapType::GrpcCallTrailingMetadata) {
        return Status::Unimplemented.as_i32();
    }
    let ctx_id = state.effective_context;
    let Some(ctx) = state.contexts.get_mut(&ctx_id) else {
        return Status::NotFound.as_i32();
    };
    if key.starts_with(':') {
        let new_val = match &m {
            HeaderMutation::Add(v) | HeaderMutation::Replace(v) => Some(v.clone()),
            HeaderMutation::Remove => None,
        };
        match (mt, key) {
            (MapType::HttpRequestHeaders, ":path") => {
                ctx.request_pseudo.path = new_val.unwrap_or_default();
            }
            (MapType::HttpRequestHeaders, ":method") => {
                ctx.request_pseudo.method = new_val.unwrap_or_default();
            }
            (MapType::HttpRequestHeaders, ":authority") => {
                ctx.request_pseudo.authority = new_val.unwrap_or_default();
            }
            (MapType::HttpRequestHeaders, ":scheme") => {
                ctx.request_pseudo.scheme = new_val.unwrap_or_default();
            }
            (MapType::HttpResponseHeaders, ":status") => {
                if let Some(v) = new_val {
                    ctx.response_status = v.parse().ok();
                }
            }
            _ => {}
        }
        return Status::Ok.as_i32();
    }
    let Ok(name) = HeaderName::try_from(key) else {
        return Status::BadArgument.as_i32();
    };
    let map = match mt {
        MapType::HttpRequestHeaders => &mut ctx.request_headers,
        MapType::HttpRequestTrailers => &mut ctx.request_trailers,
        MapType::HttpResponseHeaders => &mut ctx.response_headers,
        MapType::HttpResponseTrailers => &mut ctx.response_trailers,
        MapType::HttpCallResponseHeaders => &mut ctx.last_call_headers,
        MapType::HttpCallResponseTrailers => &mut ctx.last_call_trailers,
        MapType::GrpcCallInitialMetadata | MapType::GrpcCallTrailingMetadata => return Status::Unimplemented.as_i32(),
    };
    match m {
        HeaderMutation::Add(v) => {
            if let Ok(val) = HeaderValue::try_from(v) {
                map.append(name, val);
            }
        }
        HeaderMutation::Replace(v) => {
            if let Ok(val) = HeaderValue::try_from(v) {
                map.insert(name, val);
            }
        }
        HeaderMutation::Remove => {
            map.remove(name);
        }
    }
    Status::Ok.as_i32()
}

fn collect_pairs(state: &HostState, mt: MapType) -> Vec<(Vec<u8>, Vec<u8>)> {
    let Some(ctx) = state.current_context() else {
        return Vec::new();
    };
    let map = match mt {
        MapType::HttpRequestHeaders => &ctx.request_headers,
        MapType::HttpRequestTrailers => &ctx.request_trailers,
        MapType::HttpResponseHeaders => &ctx.response_headers,
        MapType::HttpResponseTrailers => &ctx.response_trailers,
        MapType::HttpCallResponseHeaders => &ctx.last_call_headers,
        MapType::HttpCallResponseTrailers => &ctx.last_call_trailers,
        MapType::GrpcCallInitialMetadata | MapType::GrpcCallTrailingMetadata => return Vec::new(),
    };
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(map.len() + 4);
    match mt {
        MapType::HttpRequestHeaders => {
            if !ctx.request_pseudo.method.is_empty() {
                out.push((b":method".to_vec(), ctx.request_pseudo.method.as_bytes().to_vec()));
            }
            if !ctx.request_pseudo.path.is_empty() {
                out.push((b":path".to_vec(), ctx.request_pseudo.path.as_bytes().to_vec()));
            }
            if !ctx.request_pseudo.authority.is_empty() {
                out.push((b":authority".to_vec(), ctx.request_pseudo.authority.as_bytes().to_vec()));
            }
            if !ctx.request_pseudo.scheme.is_empty() {
                out.push((b":scheme".to_vec(), ctx.request_pseudo.scheme.as_bytes().to_vec()));
            }
        }
        MapType::HttpResponseHeaders => {
            if let Some(s) = ctx.response_status {
                out.push((b":status".to_vec(), s.to_string().into_bytes()));
            }
        }
        MapType::HttpCallResponseHeaders => {
            if ctx.last_call_status > 0 {
                out.push((b":status".to_vec(), ctx.last_call_status.to_string().into_bytes()));
            }
        }
        _ => {}
    }
    for (k, v) in map.iter() {
        out.push((k.as_str().as_bytes().to_vec(), v.as_bytes().to_vec()));
    }
    out
}

fn pairs_to_header_map(pairs: &[(Vec<u8>, Vec<u8>)]) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (k, v) in pairs {
        let Ok(key) = HeaderName::try_from(k.as_slice()) else {
            continue;
        };
        let Ok(val) = HeaderValue::from_bytes(v.as_slice()) else {
            continue;
        };
        out.append(key, val);
    }
    out
}

fn replace_map(state: &mut HostState, mt: MapType, new_map: HeaderMap) {
    let ctx_id = state.effective_context;
    let Some(ctx) = state.contexts.get_mut(&ctx_id) else {
        return;
    };
    match mt {
        MapType::HttpRequestHeaders => ctx.request_headers = new_map,
        MapType::HttpRequestTrailers => ctx.request_trailers = new_map,
        MapType::HttpResponseHeaders => ctx.response_headers = new_map,
        MapType::HttpResponseTrailers => ctx.response_trailers = new_map,
        _ => {}
    }
}
