//! proxy-wasm ABI v0.2.1 的基础类型与内存/编码工具。
//!
//! 主要分三块：
//! 1. `Status` / `Action` / `MapType` / `BufferType` / `StreamType` / `MetricType` / `PeerType` /
//!    `LogLevel` 枚举（按 spec 1:1 完整覆盖）
//! 2. `MemoryHelper`：通过 `wasmtime::Memory` 安全读写 guest 线性内存
//! 3. `pairs`：proxy-wasm 头部 (k, v) 列表的二进制布局编解码
//!
//! 所有越界访问统一转 `WasmHostError::MemoryOob`，避免 trap 撕裂 Store。

use crate::error::WasmHostError;
use wasmtime::{Caller, Memory, StoreContext, StoreContextMut};

// ─────────────────────────────────────────────────────────
// 枚举：proxy-wasm v0.2.1 spec §Types
// ─────────────────────────────────────────────────────────

/// `proxy_status_t`：所有 host fn 的返回值（spec 完整 10 个值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Status {
    Ok = 0,
    NotFound = 1,
    BadArgument = 2,
    SerializationFailure = 3,
    ParseFailure = 4,
    InvalidMemoryAccess = 6,
    Empty = 7,
    CasMismatch = 8,
    InternalFailure = 10,
    Unimplemented = 12,
}

impl Status {
    #[inline]
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// `proxy_action_t`：guest 钩子返回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Action {
    Continue = 0,
    Pause = 1,
}

impl Action {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Action::Pause,
            _ => Action::Continue,
        }
    }
}

/// `proxy_map_type_t`：头部映射的来源（spec §Types 完整 8 个值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapType {
    HttpRequestHeaders = 0,
    HttpRequestTrailers = 1,
    HttpResponseHeaders = 2,
    HttpResponseTrailers = 3,
    GrpcCallInitialMetadata = 4,
    GrpcCallTrailingMetadata = 5,
    HttpCallResponseHeaders = 6,
    HttpCallResponseTrailers = 7,
}

impl MapType {
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => MapType::HttpRequestHeaders,
            1 => MapType::HttpRequestTrailers,
            2 => MapType::HttpResponseHeaders,
            3 => MapType::HttpResponseTrailers,
            4 => MapType::GrpcCallInitialMetadata,
            5 => MapType::GrpcCallTrailingMetadata,
            6 => MapType::HttpCallResponseHeaders,
            7 => MapType::HttpCallResponseTrailers,
            _ => return None,
        })
    }
}

/// `proxy_buffer_type_t`：缓冲区来源（spec §Types 完整 9 个值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferType {
    HttpRequestBody = 0,
    HttpResponseBody = 1,
    DownstreamData = 2,
    UpstreamData = 3,
    HttpCallResponseBody = 4,
    GrpcCallMessage = 5,
    VmConfiguration = 6,
    PluginConfiguration = 7,
    ForeignFunctionArguments = 8,
}

impl BufferType {
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => BufferType::HttpRequestBody,
            1 => BufferType::HttpResponseBody,
            2 => BufferType::DownstreamData,
            3 => BufferType::UpstreamData,
            4 => BufferType::HttpCallResponseBody,
            5 => BufferType::GrpcCallMessage,
            6 => BufferType::VmConfiguration,
            7 => BufferType::PluginConfiguration,
            8 => BufferType::ForeignFunctionArguments,
            _ => return None,
        })
    }
}

/// `proxy_stream_type_t`：`proxy_continue_stream` / `proxy_close_stream` 参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    HttpRequest = 0,
    HttpResponse = 1,
    Downstream = 2,
    Upstream = 3,
}

impl StreamType {
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => StreamType::HttpRequest,
            1 => StreamType::HttpResponse,
            2 => StreamType::Downstream,
            3 => StreamType::Upstream,
            _ => return None,
        })
    }
}

/// `proxy_metric_type_t`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    Counter = 0,
    Gauge = 1,
    Histogram = 2,
}

impl MetricType {
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => MetricType::Counter,
            1 => MetricType::Gauge,
            2 => MetricType::Histogram,
            _ => return None,
        })
    }
}

/// `proxy_peer_type_t`（TCP 用，暂不调用但保留类型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PeerType {
    Unknown = 0,
    Local = 1,
    Remote = 2,
}

/// `proxy_log_level_t`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Critical = 5,
}

impl LogLevel {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// `proxy_log` 的 level（tracing 转换用）。
pub fn log_level_to_tracing(level: i32) -> Option<tracing::Level> {
    Some(match level {
        0 => tracing::Level::TRACE,
        1 => tracing::Level::DEBUG,
        2 => tracing::Level::INFO,
        3 => tracing::Level::WARN,
        4 | 5 => tracing::Level::ERROR,
        _ => return None,
    })
}

/// host tracing 最大级别 → proxy_log_level_t（用于 `proxy_get_log_level`）。
pub fn host_max_log_level() -> LogLevel {
    if tracing::enabled!(tracing::Level::TRACE) {
        LogLevel::Trace
    } else if tracing::enabled!(tracing::Level::DEBUG) {
        LogLevel::Debug
    } else if tracing::enabled!(tracing::Level::INFO) {
        LogLevel::Info
    } else if tracing::enabled!(tracing::Level::WARN) {
        LogLevel::Warn
    } else {
        LogLevel::Error
    }
}

// ─────────────────────────────────────────────────────────
// WASI 常量子集
// ─────────────────────────────────────────────────────────

/// `wasi_errno_t`（spec §Types 中的子集）。
pub mod wasi_errno {
    pub const SUCCESS: i32 = 0;
    pub const BADF: i32 = 8;
    pub const FAULT: i32 = 21;
    #[allow(dead_code)]
    pub const INVAL: i32 = 28;
    #[allow(dead_code)]
    pub const NOTSUP: i32 = 58;
}

/// `wasi_fd_id_t`：stdout / stderr。
pub mod wasi_fd {
    pub const STDOUT: i32 = 1;
    pub const STDERR: i32 = 2;
}

// ─────────────────────────────────────────────────────────
// MemoryHelper：guest 内存读写（按 host fn 单次调用的生命周期使用）
// ─────────────────────────────────────────────────────────

pub struct MemoryHelper {
    memory: Memory,
}

impl MemoryHelper {
    pub fn new(memory: Memory) -> Self {
        Self { memory }
    }

    /// 从 caller 中拿到 `memory` export 的 helper（在每个 host fn 起始处调用）。
    pub fn from_caller<T>(caller: &mut Caller<'_, T>) -> Result<Self, WasmHostError> {
        let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
            return Err(WasmHostError::AbiViolation("guest module has no `memory` export".to_string()));
        };
        Ok(Self { memory: mem })
    }

    /// 读取 guest 线性内存 `[ptr, ptr+len)` 的字节切片。
    pub fn read_bytes<T>(&self, store: StoreContext<'_, T>, ptr: u32, len: u32) -> Result<Vec<u8>, WasmHostError> {
        let data = self.memory.data(&store);
        let start = ptr as usize;
        let end = start.saturating_add(len as usize);
        if end > data.len() {
            return Err(WasmHostError::MemoryOob { ptr, len });
        }
        Ok(data[start..end].to_vec())
    }

    /// 读 UTF-8 字符串；非法 UTF-8 用 lossy 转换，不报错。
    pub fn read_string_lossy<T>(&self, store: StoreContext<'_, T>, ptr: u32, len: u32) -> Result<String, WasmHostError> {
        let bytes = self.read_bytes(store, ptr, len)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// 把 host 数据写入 guest 已经分配好的 `ptr` 处。
    pub fn write_bytes<T>(&self, mut store: StoreContextMut<'_, T>, ptr: u32, data: &[u8]) -> Result<(), WasmHostError> {
        let mem = self.memory.data_mut(&mut store);
        let start = ptr as usize;
        let end = start.saturating_add(data.len());
        if end > mem.len() {
            return Err(WasmHostError::MemoryOob { ptr, len: data.len() as u32 });
        }
        mem[start..end].copy_from_slice(data);
        Ok(())
    }

    /// 读 little-endian u32。
    pub fn read_u32<T>(&self, store: StoreContext<'_, T>, ptr: u32) -> Result<u32, WasmHostError> {
        let bytes = self.read_bytes(store, ptr, 4)?;
        let arr: [u8; 4] = bytes.as_slice().try_into().map_err(|_| WasmHostError::MemoryOob { ptr, len: 4 })?;
        Ok(u32::from_le_bytes(arr))
    }

    /// 写入一个 little-endian u32 到 guest 内存。
    pub fn write_u32<T>(&self, store: StoreContextMut<'_, T>, ptr: u32, value: u32) -> Result<(), WasmHostError> {
        self.write_bytes(store, ptr, &value.to_le_bytes())
    }

    /// 写入一个 little-endian u64 到 guest 内存。
    pub fn write_u64<T>(&self, store: StoreContextMut<'_, T>, ptr: u32, value: u64) -> Result<(), WasmHostError> {
        self.write_bytes(store, ptr, &value.to_le_bytes())
    }
}

// ─────────────────────────────────────────────────────────
// header / call pairs 的二进制布局编解码
// ─────────────────────────────────────────────────────────
//
// proxy-wasm header pairs 序列化结构（little-endian）：
// ```
// u32 count
// repeat count: u32 key_size, u32 value_size
// repeat count: key_bytes, \0, value_bytes, \0
// ```
// `\0` 是为 C 互操作而保留的尾字节；rust 解码端会忽略它。
// 编码侧也按规范追加 `\0`。

pub fn encode_pairs(pairs: &[(&[u8], &[u8])]) -> Vec<u8> {
    let count = pairs.len() as u32;
    let mut cap: usize = 4 + pairs.len() * 8;
    for (k, v) in pairs {
        cap += k.len() + 1 + v.len() + 1;
    }
    let mut out = Vec::with_capacity(cap);
    out.extend_from_slice(&count.to_le_bytes());
    for (k, v) in pairs {
        out.extend_from_slice(&(k.len() as u32).to_le_bytes());
        out.extend_from_slice(&(v.len() as u32).to_le_bytes());
    }
    for (k, v) in pairs {
        out.extend_from_slice(k);
        out.push(0);
        out.extend_from_slice(v);
        out.push(0);
    }
    out
}

/// 解码 `proxy_set_header_map_pairs` 写入的字节流为 (key, value) 列表。
///
/// 严格按编码格式校验长度；不合法直接返回 `None`，由 host 端转 BadArgument。
/// 空 map 允许两种编码：空 buf（`size=0`）或单 `0x00` 字节（spec §Serialization）。
pub fn decode_pairs(bytes: &[u8]) -> Option<Vec<(Vec<u8>, Vec<u8>)>> {
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    if bytes == [0u8] {
        return Some(Vec::new());
    }
    if bytes.len() < 4 {
        return None;
    }
    let mut pos = 0;
    let count = u32_from_slice(bytes, pos)? as usize;
    pos += 4;
    if bytes.len() < 4 + count * 8 {
        return None;
    }
    let mut sizes = Vec::with_capacity(count);
    for _ in 0..count {
        let k = u32_from_slice(bytes, pos)? as usize;
        pos += 4;
        let v = u32_from_slice(bytes, pos)? as usize;
        pos += 4;
        sizes.push((k, v));
    }
    let mut out = Vec::with_capacity(count);
    for (ks, vs) in sizes {
        if pos + ks + 1 + vs + 1 > bytes.len() {
            return None;
        }
        let key = bytes[pos..pos + ks].to_vec();
        pos += ks + 1;
        let val = bytes[pos..pos + vs].to_vec();
        pos += vs + 1;
        out.push((key, val));
    }
    Some(out)
}

#[inline]
fn u32_from_slice(bytes: &[u8], pos: usize) -> Option<u32> {
    let s = bytes.get(pos..pos + 4)?;
    let arr: [u8; 4] = s.try_into().ok()?;
    Some(u32::from_le_bytes(arr))
}

/// 把 property path（`\0` 分割的多段字节流）拆成 segments。
///
/// spec §Serialization: "Host implementations should tolerate a NULL character at the end".
pub fn decode_property_path(bytes: &[u8]) -> Vec<&[u8]> {
    let trimmed = if bytes.last().copied() == Some(0) { &bytes[..bytes.len() - 1] } else { bytes };
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed.split(|b| *b == 0u8).collect()
}
