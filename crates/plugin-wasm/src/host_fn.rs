//! 把 proxy-wasm 0.2.x 的全部 host fn 注册到 `wasmtime::Linker<HostState>`。
//!
//! 实现策略：
//!
//! - 全部使用 **同步** `func_wrap`（host 端不需要 await）。
//! - `proxy_http_call` 是唯一的"异步"——它**同步**返回 token，把真正的 HTTP 调用 `tokio::spawn`
//!   出去，结果通过 `dispatch_tx` 投递回 Vm 状态机；Vm 主循环 await。
//! - hai-process-mix 没用到的能力（grpc_*、shared_data、foreign_function、queue）
//!   全部 stub 为 `Status::Unimplemented`（i32 = 12）以避免 wasmtime instantiate 失败。
//!
//! 命名与 proxy-wasm spec 完全一致；参数按 i32（线性内存偏移/长度均为 i32）。

use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use tracing::{debug, info, warn};
use wasmtime::{AsContext, AsContextMut, Caller, Linker};

use crate::abi::{decode_pairs, encode_pairs, log_level_to_tracing, BufferType, MapType, MemoryHelper, Status, StreamType};
use crate::host_state::{HostState, HttpCallResult, LocalResponse};

/// 把所有 hai-process-mix 用到的 host fn 注册到 linker。
///
/// `dispatch_tx` 用于把异步 HTTP 调用结果发送给 Vm 状态机。
pub fn register_all(
    linker: &mut Linker<HostState>,
    dispatch_tx: tokio::sync::mpsc::UnboundedSender<(u32, HttpCallResult)>,
) -> Result<(), wasmtime::Error> {
    // ─────────── proxy_log ───────────
    linker.func_wrap(
        "env",
        "proxy_log",
        |mut caller: Caller<'_, HostState>, level: i32, msg_ptr: i32, msg_size: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let msg = mem
                .read_string_lossy(caller.as_context(), msg_ptr as u32, msg_size as u32)
                .unwrap_or_default();
            let lvl = log_level_to_tracing(level);
            match lvl {
                tracing::Level::TRACE => tracing::trace!(target: "spacegate_plugin_wasm::guest", "{msg}"),
                tracing::Level::DEBUG => tracing::debug!(target: "spacegate_plugin_wasm::guest", "{msg}"),
                tracing::Level::INFO => tracing::info!(target: "spacegate_plugin_wasm::guest", "{msg}"),
                tracing::Level::WARN => tracing::warn!(target: "spacegate_plugin_wasm::guest", "{msg}"),
                tracing::Level::ERROR => tracing::error!(target: "spacegate_plugin_wasm::guest", "{msg}"),
            }
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_get_current_time_nanoseconds(return_time_ptr) ───────────
    linker.func_wrap(
        "env",
        "proxy_get_current_time_nanoseconds",
        |mut caller: Caller<'_, HostState>, return_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            if mem.write_u64(caller.as_context_mut(), return_ptr as u32, nanos).is_err() {
                return Status::InternalFailure.as_i32();
            }
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_set_tick_period_milliseconds(period) ───────────
    linker.func_wrap(
        "env",
        "proxy_set_tick_period_milliseconds",
        |mut caller: Caller<'_, HostState>, period: i32| -> i32 {
            caller.data_mut().tick_period_ms = if period > 0 { Some(period as u32) } else { None };
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_set_effective_context(context_id) ───────────
    linker.func_wrap(
        "env",
        "proxy_set_effective_context",
        |mut caller: Caller<'_, HostState>, ctx_id: i32| -> i32 {
            caller.data_mut().effective_context = ctx_id as u32;
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_done ───────────
    linker.func_wrap("env", "proxy_done", |_caller: Caller<'_, HostState>| -> i32 { Status::Ok.as_i32() })?;

    // ─────────── proxy_continue_stream(stream_type) ───────────
    //
    // hai 通过 `resume_http_request()` 调它，stream_type=0 表示 Request。
    // host 端把当前 ctx 的 continue_requested 置 true，Vm 状态机据此退出 await loop。
    linker.func_wrap(
        "env",
        "proxy_continue_stream",
        |mut caller: Caller<'_, HostState>, stream_type: i32| -> i32 {
            let st = caller.data();
            let ctx_id = st.effective_context;
            let _ = StreamType::from_i32(stream_type);
            if let Some(ctx) = caller.data_mut().contexts.get_mut(&ctx_id) {
                ctx.continue_requested = true;
            }
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_close_stream(stream_type) ───────────
    linker.func_wrap(
        "env",
        "proxy_close_stream",
        |_caller: Caller<'_, HostState>, _stream_type: i32| -> i32 { Status::Ok.as_i32() },
    )?;

    // ─────────── proxy_get_buffer_bytes ───────────
    //
    // 签名：(buffer_type, start, max_size, return_data_ptr, return_size_ptr) -> Status
    // host 端要：
    // 1. 从 HostState 拿对应 buffer（plugin_config / request_body / response_body / call_response_body）
    // 2. 调 guest 的 `proxy_on_memory_allocate` 让它给一块缓冲
    // 3. 把字节写到 guest 内存，写回 *return_data = ptr, *return_size = len
    linker.func_wrap(
        "env",
        "proxy_get_buffer_bytes",
        |mut caller: Caller<'_, HostState>,
         buffer_type: i32,
         start: i32,
         max_size: i32,
         return_data_ptr: i32,
         return_size_ptr: i32|
         -> i32 {
            let buf_type = BufferType::from_i32(buffer_type);
            let bytes_opt: Option<Vec<u8>> = match buf_type {
                BufferType::PluginConfiguration | BufferType::VmConfiguration => Some(caller.data().configuration.clone()),
                BufferType::HttpRequestBody => caller
                    .data()
                    .current_context()
                    .and_then(|c| c.request_body.as_ref().map(|b| b.to_vec())),
                BufferType::HttpResponseBody => caller
                    .data()
                    .current_context()
                    .and_then(|c| c.response_body.as_ref().map(|b| b.to_vec())),
                BufferType::HttpCallResponseBody => caller
                    .data()
                    .current_context()
                    .map(|c| c.last_call_body.to_vec()),
                BufferType::Unknown(_) => None,
            };
            let bytes = match bytes_opt {
                Some(b) => b,
                None => return Status::NotFound.as_i32(),
            };
            // 截取 [start, start + max_size)；max_size 是 u32 reinterpret 进来的，
            // proxy-wasm-rust-sdk 经常传 usize::MAX -> u32::MAX，所以这里按 u32 重新解释。
            let start = (start as u32) as usize;
            let max_size = (max_size as u32) as usize;
            if start > bytes.len() {
                return Status::BadArgument.as_i32();
            }
            let end = (start.saturating_add(max_size)).min(bytes.len());
            let slice = &bytes[start..end];
            // 空 buffer：写回 (0, 0) 并返回 Ok，让 guest 知道存在但是 0 长度
            if slice.is_empty() {
                let mem = match MemoryHelper::from_caller(&mut caller) {
                    Ok(m) => m,
                    Err(_) => return Status::InternalFailure.as_i32(),
                };
                let _ = mem.write_u32(caller.as_context_mut(), return_data_ptr as u32, 0);
                let _ = mem.write_u32(caller.as_context_mut(), return_size_ptr as u32, 0);
                return Status::Ok.as_i32();
            }
            // 让 guest 分配
            let alloc = match caller.data().alloc.clone() {
                Some(f) => f,
                None => return Status::InternalFailure.as_i32(),
            };
            let guest_ptr = match alloc.call(&mut caller, slice.len() as u32) {
                Ok(p) => p,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            if mem.write_bytes(caller.as_context_mut(), guest_ptr, slice).is_err() {
                return Status::InternalFailure.as_i32();
            }
            let _ = mem.write_u32(caller.as_context_mut(), return_data_ptr as u32, guest_ptr);
            let _ = mem.write_u32(caller.as_context_mut(), return_size_ptr as u32, slice.len() as u32);
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_set_buffer_bytes ───────────
    //
    // 用于 guest 写回 response body（流式 hai 才会用，本阶段不实现完整流式，但 spec 要求接口存在）。
    linker.func_wrap(
        "env",
        "proxy_set_buffer_bytes",
        |mut caller: Caller<'_, HostState>,
         buffer_type: i32,
         _start: i32,
         _size: i32,
         data_ptr: i32,
         data_size: i32|
         -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let bytes = match mem.read_bytes(caller.as_context(), data_ptr as u32, data_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            let bt = BufferType::from_i32(buffer_type);
            let ctx_id = caller.data().effective_context;
            if let Some(ctx) = caller.data_mut().contexts.get_mut(&ctx_id) {
                match bt {
                    BufferType::HttpRequestBody => ctx.request_body = Some(Bytes::from(bytes)),
                    BufferType::HttpResponseBody => ctx.response_body = Some(Bytes::from(bytes)),
                    _ => return Status::BadArgument.as_i32(),
                }
            }
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_get_header_map_value ───────────
    linker.func_wrap(
        "env",
        "proxy_get_header_map_value",
        |mut caller: Caller<'_, HostState>,
         map_type: i32,
         key_ptr: i32,
         key_size: i32,
         return_data_ptr: i32,
         return_size_ptr: i32|
         -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let key = match mem.read_string_lossy(caller.as_context(), key_ptr as u32, key_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            let key_l = key.to_ascii_lowercase();
            let mt = MapType::from_i32(map_type);
            let value_opt = lookup_header(caller.data(), mt, &key_l);
            let Some(value) = value_opt else {
                return Status::NotFound.as_i32();
            };
            let bytes = value.into_bytes();
            // 空字符串也要分配 0 长度
            let alloc = match caller.data().alloc.clone() {
                Some(f) => f,
                None => return Status::InternalFailure.as_i32(),
            };
            let guest_ptr = if bytes.is_empty() {
                0
            } else {
                match alloc.call(&mut caller, bytes.len() as u32) {
                    Ok(p) => p,
                    Err(_) => return Status::InternalFailure.as_i32(),
                }
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            if guest_ptr > 0 {
                if mem.write_bytes(caller.as_context_mut(), guest_ptr, &bytes).is_err() {
                    return Status::InternalFailure.as_i32();
                }
            }
            let _ = mem.write_u32(caller.as_context_mut(), return_data_ptr as u32, guest_ptr);
            let _ = mem.write_u32(caller.as_context_mut(), return_size_ptr as u32, bytes.len() as u32);
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_add_header_map_value ───────────
    linker.func_wrap(
        "env",
        "proxy_add_header_map_value",
        |mut caller: Caller<'_, HostState>,
         map_type: i32,
         key_ptr: i32,
         key_size: i32,
         value_ptr: i32,
         value_size: i32|
         -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let key = match mem.read_string_lossy(caller.as_context(), key_ptr as u32, key_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            let value = match mem.read_string_lossy(caller.as_context(), value_ptr as u32, value_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            mutate_header(caller.data_mut(), MapType::from_i32(map_type), &key, HeaderMutation::Add(value))
        },
    )?;

    // ─────────── proxy_replace_header_map_value ───────────
    linker.func_wrap(
        "env",
        "proxy_replace_header_map_value",
        |mut caller: Caller<'_, HostState>,
         map_type: i32,
         key_ptr: i32,
         key_size: i32,
         value_ptr: i32,
         value_size: i32|
         -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let key = match mem.read_string_lossy(caller.as_context(), key_ptr as u32, key_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            let value = match mem.read_string_lossy(caller.as_context(), value_ptr as u32, value_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            mutate_header(caller.data_mut(), MapType::from_i32(map_type), &key, HeaderMutation::Replace(value))
        },
    )?;

    // ─────────── proxy_remove_header_map_value ───────────
    linker.func_wrap(
        "env",
        "proxy_remove_header_map_value",
        |mut caller: Caller<'_, HostState>, map_type: i32, key_ptr: i32, key_size: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let key = match mem.read_string_lossy(caller.as_context(), key_ptr as u32, key_size as u32) {
                Ok(s) => s,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            mutate_header(caller.data_mut(), MapType::from_i32(map_type), &key, HeaderMutation::Remove)
        },
    )?;

    // ─────────── proxy_get_header_map_pairs ───────────
    linker.func_wrap(
        "env",
        "proxy_get_header_map_pairs",
        |mut caller: Caller<'_, HostState>, map_type: i32, return_data_ptr: i32, return_size_ptr: i32| -> i32 {
            let mt = MapType::from_i32(map_type);
            let pairs = collect_pairs(caller.data(), mt);
            let buf = {
                let refs: Vec<(&[u8], &[u8])> = pairs.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();
                encode_pairs(&refs)
            };
            let alloc = match caller.data().alloc.clone() {
                Some(f) => f,
                None => return Status::InternalFailure.as_i32(),
            };
            let guest_ptr = match alloc.call(&mut caller, buf.len() as u32) {
                Ok(p) => p,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            if mem.write_bytes(caller.as_context_mut(), guest_ptr, &buf).is_err() {
                return Status::InternalFailure.as_i32();
            }
            let _ = mem.write_u32(caller.as_context_mut(), return_data_ptr as u32, guest_ptr);
            let _ = mem.write_u32(caller.as_context_mut(), return_size_ptr as u32, buf.len() as u32);
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_set_header_map_pairs ───────────
    linker.func_wrap(
        "env",
        "proxy_set_header_map_pairs",
        |mut caller: Caller<'_, HostState>, map_type: i32, data_ptr: i32, data_size: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let raw = match mem.read_bytes(caller.as_context(), data_ptr as u32, data_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            let Some(pairs) = decode_pairs(&raw) else {
                return Status::BadArgument.as_i32();
            };
            let mt = MapType::from_i32(map_type);
            let new_map = pairs_to_header_map(&pairs);
            replace_map(caller.data_mut(), mt, new_map);
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_get_property ───────────
    //
    // hai 用它读 `source.address`（客户端 IP）。我们把请求里能拿到的 source ip 提前
    // 放到 ctx.request_pseudo 或 properties 表里，host fn 这里检索。
    linker.func_wrap(
        "env",
        "proxy_get_property",
        |mut caller: Caller<'_, HostState>, path_ptr: i32, path_size: i32, return_data_ptr: i32, return_size_ptr: i32| -> i32 {
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let raw = match mem.read_bytes(caller.as_context(), path_ptr as u32, path_size as u32) {
                Ok(b) => b,
                Err(_) => return Status::BadArgument.as_i32(),
            };
            // path 是用 '\0' 分割的多段（proxy-wasm 约定）
            let segments: Vec<&[u8]> = raw.split(|b| *b == 0u8).filter(|s| !s.is_empty()).collect();
            // 我们暂时仅识别 `source.address` 一种（hai 唯一用例）。
            let value: Option<Vec<u8>> = if segments == [b"source".as_slice(), b"address".as_slice()] {
                // 优先从 :authority / x-forwarded-for 推导（无客户端 socket 信息时降级）
                caller
                    .data()
                    .current_context()
                    .and_then(|c| {
                        if !c.request_pseudo.authority.is_empty() {
                            Some(c.request_pseudo.authority.clone())
                        } else {
                            None
                        }
                    })
                    .map(|s| s.into_bytes())
            } else {
                None
            };
            let Some(bytes) = value else {
                return Status::NotFound.as_i32();
            };
            let alloc = match caller.data().alloc.clone() {
                Some(f) => f,
                None => return Status::InternalFailure.as_i32(),
            };
            let guest_ptr = match alloc.call(&mut caller, bytes.len() as u32) {
                Ok(p) => p,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let mem = match MemoryHelper::from_caller(&mut caller) {
                Ok(m) => m,
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            if mem.write_bytes(caller.as_context_mut(), guest_ptr, &bytes).is_err() {
                return Status::InternalFailure.as_i32();
            }
            let _ = mem.write_u32(caller.as_context_mut(), return_data_ptr as u32, guest_ptr);
            let _ = mem.write_u32(caller.as_context_mut(), return_size_ptr as u32, bytes.len() as u32);
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_set_property ───────────（stub）
    linker.func_wrap(
        "env",
        "proxy_set_property",
        |_caller: Caller<'_, HostState>, _p_ptr: i32, _p_size: i32, _v_ptr: i32, _v_size: i32| -> i32 {
            Status::Ok.as_i32()
        },
    )?;

    // ─────────── proxy_send_local_response ───────────
    //
    // 签名：(status, status_text_data, status_text_size, body_data, body_size,
    //        additional_headers_data, additional_headers_size, grpc_status) -> Status
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
                Err(_) => return Status::InternalFailure.as_i32(),
            };
            let body = if body_size > 0 {
                mem.read_bytes(caller.as_context(), body_data as u32, body_size as u32)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let headers_bytes = if headers_size > 0 {
                mem.read_bytes(caller.as_context(), headers_data as u32, headers_size as u32)
                    .unwrap_or_default()
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

    // ─────────── proxy_http_call ───────────
    //
    // 签名（type 15）：
    // (upstream_data, upstream_size, headers_data, headers_size,
    //  body_data, body_size, trailers_data, trailers_size, timeout_ms, return_token_ptr) -> Status
    linker.func_wrap(
        "env",
        "proxy_http_call",
        {
            let dispatch_tx = dispatch_tx.clone();
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
                    Err(_) => return Status::InternalFailure.as_i32(),
                };
                let cluster = match mem.read_string_lossy(caller.as_context(), upstream_data as u32, upstream_size as u32) {
                    Ok(s) => s,
                    Err(_) => return Status::BadArgument.as_i32(),
                };
                let headers_bytes = mem.read_bytes(caller.as_context(), headers_data as u32, headers_size as u32).unwrap_or_default();
                let body = if body_size > 0 {
                    mem.read_bytes(caller.as_context(), body_data as u32, body_size as u32).unwrap_or_default()
                } else {
                    Vec::new()
                };
                let pairs = decode_pairs(&headers_bytes).unwrap_or_default();
                // 解析 :method / :path / :authority
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
                        ":scheme" => { /* host 层只用 http */ }
                        _ => others.push((key_str.to_string(), val_str)),
                    }
                }
                // cluster → base URL
                let st = caller.data();
                let base = st.shell_cfg.resolve_cluster(&cluster).or_else(|| {
                    if !authority.is_empty() {
                        Some(format!("http://{authority}"))
                    } else {
                        None
                    }
                });
                let Some(base) = base else {
                    warn!(target: "spacegate_plugin_wasm", cluster = %cluster, "dispatch_http_call: cluster not configured");
                    return Status::BadArgument.as_i32();
                };
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
                let timeout = Duration::from_millis(timeout_ms.max(1) as u64);
                let tx = dispatch_tx.clone();
                tokio::spawn(async move {
                    debug!(target: "spacegate_plugin_wasm", %url, %method, "dispatch_http_call begin");
                    let parsed_method = match method.parse::<reqwest::Method>() {
                        Ok(m) => m,
                        Err(_) => reqwest::Method::GET,
                    };
                    let mut req = client.request(parsed_method, &url);
                    for (k, v) in others {
                        // 跳过 hop-by-hop / 非法字符头
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
                            let mut hdrs = HeaderMap::new();
                            for (k, v) in resp.headers().iter() {
                                if let (Ok(name), Ok(val)) =
                                    (HeaderName::try_from(k.as_str()), HeaderValue::from_bytes(v.as_bytes()))
                                {
                                    hdrs.append(name, val);
                                }
                            }
                            let body_bytes = resp.bytes().await.unwrap_or_default();
                            HttpCallResult {
                                status,
                                headers: hdrs,
                                body: body_bytes,
                            }
                        }
                        Err(e) => {
                            warn!(target: "spacegate_plugin_wasm", %url, error = %e, "dispatch_http_call failed");
                            HttpCallResult {
                                status: 0,
                                headers: HeaderMap::new(),
                                body: Bytes::new(),
                            }
                        }
                    };
                    debug!(target: "spacegate_plugin_wasm", token, status = result.status, "dispatch_http_call done");
                    let _ = tx.send((token, result));
                });
                // 写回 token
                let mem = match MemoryHelper::from_caller(&mut caller) {
                    Ok(m) => m,
                    Err(_) => return Status::InternalFailure.as_i32(),
                };
                let _ = mem.write_u32(caller.as_context_mut(), return_token_ptr as u32, token);
                info!(target: "spacegate_plugin_wasm", token, cluster = %cluster, "dispatch_http_call enqueued");
                Status::Ok.as_i32()
            }
        },
    )?;

    // ─────────── 其余 hai-process-mix 模块声明但不用的 host fn，全部 stub 为 Ok ───────────
    //
    // wasmtime 要求 instantiate 时所有 import 都已 link；这里返回 Ok 不影响功能。
    stub_all_unused(linker)?;

    Ok(())
}

// ─────────────────────────────────────────────────────────
// 辅助：lookup / mutate / collect
// ─────────────────────────────────────────────────────────

fn lookup_header(state: &HostState, mt: MapType, key_lower: &str) -> Option<String> {
    let ctx = state.current_context()?;
    let map = match mt {
        MapType::HttpRequestHeaders | MapType::HttpRequestTrailers => &ctx.request_headers,
        MapType::HttpResponseHeaders | MapType::HttpResponseTrailers => &ctx.response_headers,
        MapType::HttpCallResponseHeaders | MapType::HttpCallResponseTrailers => &ctx.last_call_headers,
        MapType::Unknown(_) => return None,
    };
    // 伪头特判：:status / :method / :path / :authority / :scheme
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
    let ctx_id = state.effective_context;
    let Some(ctx) = state.contexts.get_mut(&ctx_id) else {
        return Status::NotFound.as_i32();
    };
    // 伪头处理（hai 不会改 :path 但理论上要支持）
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
        MapType::HttpRequestHeaders | MapType::HttpRequestTrailers => &mut ctx.request_headers,
        MapType::HttpResponseHeaders | MapType::HttpResponseTrailers => &mut ctx.response_headers,
        MapType::HttpCallResponseHeaders | MapType::HttpCallResponseTrailers => &mut ctx.last_call_headers,
        MapType::Unknown(_) => return Status::BadArgument.as_i32(),
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
        MapType::HttpRequestHeaders | MapType::HttpRequestTrailers => &ctx.request_headers,
        MapType::HttpResponseHeaders | MapType::HttpResponseTrailers => &ctx.response_headers,
        MapType::HttpCallResponseHeaders | MapType::HttpCallResponseTrailers => &ctx.last_call_headers,
        MapType::Unknown(_) => return Vec::new(),
    };
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(map.len() + 4);
    // 加伪头
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
        MapType::HttpResponseHeaders => ctx.response_headers = new_map,
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────
// stub：hai 模块声明但本阶段不需要语义的 host fn
// ─────────────────────────────────────────────────────────

fn stub_all_unused(linker: &mut Linker<HostState>) -> Result<(), wasmtime::Error> {
    // proxy_get_shared_data / proxy_set_shared_data：暂返回 NotFound / Ok
    linker.func_wrap(
        "env",
        "proxy_get_shared_data",
        |_caller: Caller<'_, HostState>, _k_ptr: i32, _k_size: i32, _v_ptr: i32, _v_size: i32, _cas_ptr: i32| -> i32 {
            Status::NotFound.as_i32()
        },
    )?;
    linker.func_wrap(
        "env",
        "proxy_set_shared_data",
        |_caller: Caller<'_, HostState>, _k_ptr: i32, _k_size: i32, _v_ptr: i32, _v_size: i32, _cas: i32| -> i32 {
            Status::Ok.as_i32()
        },
    )?;

    // proxy_get_status：返回 Ok（与本地响应状态码相关，但 hai 不读取）
    linker.func_wrap(
        "env",
        "proxy_get_status",
        |_caller: Caller<'_, HostState>, _status_code_ptr: i32, _msg_ptr: i32, _msg_size: i32| -> i32 { Status::Ok.as_i32() },
    )?;

    // 共享队列
    linker.func_wrap(
        "env",
        "proxy_register_shared_queue",
        |_caller: Caller<'_, HostState>, _n_ptr: i32, _n_size: i32, _ret: i32| -> i32 { Status::Empty.as_i32() },
    )?;
    linker.func_wrap(
        "env",
        "proxy_resolve_shared_queue",
        |_caller: Caller<'_, HostState>, _vid_ptr: i32, _vid_size: i32, _n_ptr: i32, _n_size: i32, _ret: i32| -> i32 {
            Status::Empty.as_i32()
        },
    )?;
    linker.func_wrap(
        "env",
        "proxy_enqueue_shared_queue",
        |_caller: Caller<'_, HostState>, _qid: i32, _v_ptr: i32, _v_size: i32| -> i32 { Status::Empty.as_i32() },
    )?;
    linker.func_wrap(
        "env",
        "proxy_dequeue_shared_queue",
        |_caller: Caller<'_, HostState>, _qid: i32, _v_ptr: i32, _v_size: i32| -> i32 { Status::Empty.as_i32() },
    )?;

    // gRPC 相关全部 Empty
    linker.func_wrap(
        "env",
        "proxy_grpc_call",
        |_caller: Caller<'_, HostState>,
         _a: i32,
         _b: i32,
         _c: i32,
         _d: i32,
         _e: i32,
         _f: i32,
         _g: i32,
         _h: i32,
         _i: i32,
         _j: i32,
         _k: i32,
         _l: i32|
         -> i32 { Status::Empty.as_i32() },
    )?;
    linker.func_wrap(
        "env",
        "proxy_grpc_stream",
        |_caller: Caller<'_, HostState>,
         _a: i32,
         _b: i32,
         _c: i32,
         _d: i32,
         _e: i32,
         _f: i32,
         _g: i32,
         _h: i32,
         _i: i32|
         -> i32 { Status::Empty.as_i32() },
    )?;
    linker.func_wrap(
        "env",
        "proxy_grpc_cancel",
        |_caller: Caller<'_, HostState>, _t: i32| -> i32 { Status::Empty.as_i32() },
    )?;
    linker.func_wrap(
        "env",
        "proxy_grpc_close",
        |_caller: Caller<'_, HostState>, _t: i32| -> i32 { Status::Empty.as_i32() },
    )?;
    linker.func_wrap(
        "env",
        "proxy_grpc_send",
        |_caller: Caller<'_, HostState>, _t: i32, _m: i32, _ms: i32, _eos: i32| -> i32 { Status::Empty.as_i32() },
    )?;

    // foreign function：不支持
    linker.func_wrap(
        "env",
        "proxy_call_foreign_function",
        |_caller: Caller<'_, HostState>, _a: i32, _b: i32, _c: i32, _d: i32, _e: i32, _f: i32| -> i32 {
            Status::Empty.as_i32()
        },
    )?;

    Ok(())
}
