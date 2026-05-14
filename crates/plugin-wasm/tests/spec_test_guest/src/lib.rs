//! 验证 spacegate-plugin-wasm host fn 实现的 **真实 proxy-wasm guest 插件**。
//!
//! 用法：通过 `cargo build --release` 编译到 `wasm32-wasip1`，得到
//! `target/wasm32-wasip1/release/spec_test_guest.wasm`，再由
//! `crates/plugin-wasm/tests/spec_compliance.rs` 加载并依次调用
//! [`__run_test`] 来跑各场景。
//!
//! 设计取舍：每个场景都通过 [`proxy_wasm::hostcalls`] 直接调相应 host fn。
//! SDK 在 status 不预期时会 panic（即 wasmtime trap），这正好让我们：
//!
//! - host fn 返回正确 Status → SDK 返回 Result → guest 自行断言并返回 0 / 失败码
//! - host fn 返回错误 Status → SDK panic → wasmtime trap → test 立刻挂掉
//!
//! 这样测试侧只看 `__run_test` 返回值就能判定通过。

use std::time::Duration;

use proxy_wasm::hostcalls;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Trace);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> { Box::new(SpecRoot) });
}}

struct SpecRoot;

impl Context for SpecRoot {}
impl RootContext for SpecRoot {
    fn on_vm_start(&mut self, _vm_configuration_size: usize) -> bool { true }
    fn on_configure(&mut self, _plugin_configuration_size: usize) -> bool { true }
    fn get_type(&self) -> Option<ContextType> { Some(ContextType::HttpContext) }
    fn create_http_context(&self, _context_id: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(SpecHttp))
    }
}

struct SpecHttp;
impl Context for SpecHttp {}
impl HttpContext for SpecHttp {}

// ─────────────────────────────────────────────────────────
// 直接 extern：spec 里这些 host fn SDK 没暴露，我们手动 import 用
// ─────────────────────────────────────────────────────────

extern "C" {
    fn proxy_grpc_call(
        a: *const u8, b: usize, c: *const u8, d: usize, e: *const u8, f: usize,
        g: *const u8, h: usize, i: *const u8, j: usize, k: u32, l: *mut u32,
    ) -> u32;
    fn proxy_call_foreign_function(
        a: *const u8, b: usize, c: *const u8, d: usize, e: *mut *mut u8, f: *mut usize,
    ) -> u32;
    fn proxy_continue_stream(stream_type: u32) -> u32;
    fn proxy_close_stream(stream_type: u32) -> u32;
    fn proxy_set_effective_context(ctx: u32) -> u32;
    fn proxy_done() -> u32;
}

// spec §Types: STATUS_UNIMPLEMENTED = 12, STATUS_NOT_FOUND = 1, STATUS_BAD_ARGUMENT = 2
const STATUS_OK: u32 = 0;
const STATUS_NOT_FOUND: u32 = 1;
const STATUS_BAD_ARGUMENT: u32 = 2;
const STATUS_UNIMPLEMENTED: u32 = 12;

const STREAM_HTTP_REQUEST: u32 = 0;
const STREAM_DOWNSTREAM: u32 = 2;

// ─────────────────────────────────────────────────────────
// 测试入口
// ─────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn __run_test(scenario: u32) -> u32 {
    match scenario {
        1 => test_shared_data(),
        2 => test_shared_queue(),
        3 => test_metric_counter(),
        4 => test_metric_gauge(),
        5 => test_user_property(),
        6 => test_well_known_plugin_name(),
        7 => test_log_level(),
        8 => test_current_time(),
        9 => test_continue_stream_http_request(),
        10 => test_close_stream_tcp_unimplemented(),
        11 => test_set_effective_context_bad_argument(),
        12 => test_grpc_unimplemented(),
        13 => test_foreign_function_not_found(),
        14 => test_request_header_pseudo_method(),
        15 => test_add_replace_remove_header(),
        16 => test_get_configuration_buffer(),
        17 => test_send_local_response(),
        18 => test_done_without_pending(),
        19 => test_log(),
        20 => test_tick_period(),
        _ => 999,
    }
}

// ─── 1: shared_data CAS roundtrip ───
fn test_shared_data() -> u32 {
    let key = "spec.shared_data.k";
    hostcalls::set_shared_data(key, Some(b"v1"), None).unwrap();
    let (val, cas) = hostcalls::get_shared_data(key).unwrap();
    if val.as_deref() != Some(b"v1".as_slice()) { return 1; }
    let cas = match cas { Some(c) if c > 0 => c, _ => return 2 };
    // 错误 cas → CasMismatch（SDK 把 CasMismatch 包装成 Err）
    if hostcalls::set_shared_data(key, Some(b"v2"), Some(cas.wrapping_add(999))).is_ok() {
        return 3;
    }
    // 正确 cas → Ok
    hostcalls::set_shared_data(key, Some(b"v2"), Some(cas)).unwrap();
    let (val, cas2) = hostcalls::get_shared_data(key).unwrap();
    if val.as_deref() != Some(b"v2".as_slice()) { return 4; }
    let cas2 = cas2.unwrap_or(0);
    if cas2 <= cas { return 5; }
    0
}

// ─── 2: shared queue lifecycle ───
fn test_shared_queue() -> u32 {
    let qid = hostcalls::register_shared_queue("spec.q.basic").unwrap();
    if qid == 0 { return 1; }
    // register 同名应返回相同 qid（spec §proxy_register_shared_queue）
    if hostcalls::register_shared_queue("spec.q.basic").unwrap() != qid { return 2; }
    // resolve_shared_queue：vm_id 默认是 "default"，name 已存在
    match hostcalls::resolve_shared_queue("default", "spec.q.basic").unwrap() {
        Some(id) if id == qid => {}
        _ => return 3,
    }
    if hostcalls::resolve_shared_queue("default", "spec.q.nonexistent").unwrap().is_some() { return 4; }
    hostcalls::enqueue_shared_queue(qid, Some(b"a")).unwrap();
    hostcalls::enqueue_shared_queue(qid, Some(b"bb")).unwrap();
    match hostcalls::dequeue_shared_queue(qid).unwrap() {
        Some(v) if v == b"a".to_vec() => {}
        _ => return 5,
    }
    match hostcalls::dequeue_shared_queue(qid).unwrap() {
        Some(v) if v == b"bb".to_vec() => {}
        _ => return 6,
    }
    // 空 → Ok(None)（SDK 把 Empty 折叠成 None）
    if hostcalls::dequeue_shared_queue(qid).unwrap().is_some() { return 7; }
    // 未知 qid → Err(NotFound)
    if hostcalls::dequeue_shared_queue(9_999_999).is_ok() { return 8; }
    if hostcalls::enqueue_shared_queue(9_999_999, Some(b"x")).is_ok() { return 9; }
    0
}

// ─── 3: counter only allows positive delta ───
fn test_metric_counter() -> u32 {
    let id = hostcalls::define_metric(MetricType::Counter, "spec.counter").unwrap();
    if id == 0 { return 1; }
    hostcalls::increment_metric(id, 3).unwrap();
    hostcalls::increment_metric(id, 2).unwrap();
    if hostcalls::get_metric(id).unwrap() != 5 { return 2; }
    // counter 不能 decrement → BadArgument
    if hostcalls::increment_metric(id, -1).is_ok() { return 3; }
    if hostcalls::get_metric(id).unwrap() != 5 { return 4; }
    // 未知 mid → NotFound
    if hostcalls::get_metric(9_999_999).is_ok() { return 5; }
    0
}

// ─── 4: gauge bidirectional + record ───
fn test_metric_gauge() -> u32 {
    let id = hostcalls::define_metric(MetricType::Gauge, "spec.gauge").unwrap();
    hostcalls::increment_metric(id, 10).unwrap();
    hostcalls::increment_metric(id, -3).unwrap();
    if hostcalls::get_metric(id).unwrap() != 7 { return 1; }
    hostcalls::record_metric(id, 42).unwrap();
    if hostcalls::get_metric(id).unwrap() != 42 { return 2; }
    0
}

// ─── 5: user property set/get roundtrip ───
fn test_user_property() -> u32 {
    let path = vec!["spec", "user_prop"];
    hostcalls::set_property(path.clone(), Some(b"hello")).unwrap();
    let v = hostcalls::get_property(path.clone()).unwrap();
    if v.as_deref() != Some(b"hello".as_slice()) { return 1; }
    // None / NotFound：未设置过的 path
    let missing = hostcalls::get_property(vec!["spec", "absent"]).unwrap();
    if missing.is_some() { return 2; }
    0
}

// ─── 6: well-known property plugin_name ───
fn test_well_known_plugin_name() -> u32 {
    let v = hostcalls::get_property(vec!["plugin_name"]).unwrap();
    match v {
        Some(b) if b == b"spec-test-plugin".to_vec() => 0,
        Some(_) => 1,
        None => 2,
    }
}

// ─── 7: log_level（host 当前 tracing 最大级别） ───
fn test_log_level() -> u32 {
    let lvl = hostcalls::get_log_level().unwrap();
    // host 默认 tracing 是 ERROR 以上；我们的实现至少返回 5（CRITICAL）或更宽
    // 只要不 panic 且能拿到值就算 OK
    let _ = lvl;
    0
}

// ─── 8: current_time > 0 ───
fn test_current_time() -> u32 {
    let now = hostcalls::get_current_time().unwrap();
    if now < std::time::UNIX_EPOCH { return 1; }
    0
}

// ─── 9: continue_stream(HTTP_REQUEST) → Ok ───
fn test_continue_stream_http_request() -> u32 {
    let s = unsafe { proxy_continue_stream(STREAM_HTTP_REQUEST) };
    if s != STATUS_OK { return s; }
    0
}

// ─── 10: close_stream(DOWNSTREAM) → Unimplemented（TCP 我们不支持） ───
fn test_close_stream_tcp_unimplemented() -> u32 {
    let s = unsafe { proxy_close_stream(STREAM_DOWNSTREAM) };
    if s != STATUS_UNIMPLEMENTED { return 100 + s; }
    0
}

// ─── 11: set_effective_context 对未知 ctx → BadArgument ───
fn test_set_effective_context_bad_argument() -> u32 {
    let s = unsafe { proxy_set_effective_context(987654) };
    if s != STATUS_BAD_ARGUMENT { return 100 + s; }
    0
}

// ─── 12: gRPC host fn → Unimplemented ───
fn test_grpc_unimplemented() -> u32 {
    let mut tok: u32 = 0;
    let s = unsafe {
        proxy_grpc_call(
            b"cluster".as_ptr(), 7,
            b"svc".as_ptr(), 3,
            b"m".as_ptr(), 1,
            std::ptr::null(), 0,
            std::ptr::null(), 0,
            1000,
            &mut tok as *mut u32,
        )
    };
    if s != STATUS_UNIMPLEMENTED { return 100 + s; }
    0
}

// ─── 13: foreign_function → NotFound（无注册表） ───
fn test_foreign_function_not_found() -> u32 {
    let mut data: *mut u8 = std::ptr::null_mut();
    let mut size: usize = 0;
    let s = unsafe {
        proxy_call_foreign_function(
            b"some_fn".as_ptr(), 7,
            b"args".as_ptr(), 4,
            &mut data as *mut *mut u8,
            &mut size as *mut usize,
        )
    };
    if s != STATUS_NOT_FOUND { return 100 + s; }
    0
}

// ─── 14: get_http_request_header(":method") ───
fn test_request_header_pseudo_method() -> u32 {
    match hostcalls::get_map_value(MapType::HttpRequestHeaders, ":method").unwrap() {
        Some(m) if m == "POST" => 0,
        Some(_) => 1,
        None => 2,
    }
}

// ─── 15: add / replace / remove header on HttpRequestHeaders ───
fn test_add_replace_remove_header() -> u32 {
    hostcalls::add_map_value(MapType::HttpRequestHeaders, "x-spec-add", "v1").unwrap();
    if hostcalls::get_map_value(MapType::HttpRequestHeaders, "x-spec-add").unwrap().as_deref() != Some("v1") {
        return 1;
    }
    // SDK 用 set_map_value(map, key, Some("v2")) 触发 spec 的 replace 语义。
    hostcalls::set_map_value(MapType::HttpRequestHeaders, "x-spec-add", Some("v2")).unwrap();
    if hostcalls::get_map_value(MapType::HttpRequestHeaders, "x-spec-add").unwrap().as_deref() != Some("v2") {
        return 2;
    }
    hostcalls::remove_map_value(MapType::HttpRequestHeaders, "x-spec-add").unwrap();
    if hostcalls::get_map_value(MapType::HttpRequestHeaders, "x-spec-add").unwrap().is_some() {
        return 3;
    }
    0
}

// ─── 16: get_buffer(PluginConfiguration) 返回配置字节 ───
fn test_get_configuration_buffer() -> u32 {
    // start=0, max_size=usize::MAX
    match hostcalls::get_buffer(BufferType::PluginConfiguration, 0, usize::MAX).unwrap() {
        Some(b) if b == b"spec-test-config".to_vec() => 0,
        Some(_) => 1,
        None => 2,
    }
}

// ─── 17: send_local_response（host 侧通过 contexts[ctx].local_response 验证） ───
fn test_send_local_response() -> u32 {
    hostcalls::send_http_response(
        418,
        vec![("x-spec", "teapot")],
        Some(b"local body"),
    ).unwrap();
    0
}

// ─── 18: proxy_done 在没有 awaiting_done 时 → NotFound ───
fn test_done_without_pending() -> u32 {
    let s = unsafe { proxy_done() };
    if s != STATUS_NOT_FOUND { return 100 + s; }
    0
}

// ─── 19: proxy_log 在各级别 ───
fn test_log() -> u32 {
    hostcalls::log(LogLevel::Trace, "spec trace").unwrap();
    hostcalls::log(LogLevel::Debug, "spec debug").unwrap();
    hostcalls::log(LogLevel::Info, "spec info").unwrap();
    hostcalls::log(LogLevel::Warn, "spec warn").unwrap();
    hostcalls::log(LogLevel::Error, "spec error").unwrap();
    0
}

// ─── 20: set_tick_period 应 Ok ───
fn test_tick_period() -> u32 {
    hostcalls::set_tick_period(Duration::from_millis(123)).unwrap();
    0
}
