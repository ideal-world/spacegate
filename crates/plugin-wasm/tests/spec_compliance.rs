//! End-to-end spec compliance test：用 `proxy-wasm-rust-sdk` 编出来的真实 guest 插件
//! ([`crates/plugin-wasm/tests/spec_test_guest`])  跑一遍我们 host 注册的所有 hostcall，
//! 覆盖 proxy-wasm v0.2.1 spec 关键面：
//!
//! - Shared K/V（带 CAS）
//! - Shared queues（含 register/resolve/enqueue/dequeue 全链）
//! - Metrics（counter / gauge / record / increment）
//! - Properties（user + well-known `plugin_name`）
//! - Logging / Clocks
//! - Buffer（PluginConfiguration）
//! - HTTP header map（含 `:method` 伪头）
//! - Stream control（continue / close）
//! - effective_context / done
//! - gRPC / foreign_function 的 spec 合规返回值
//! - send_local_response 短路写入
//! - set_tick_period 接收
//!
//! 运行：先 `cd crates/plugin-wasm/tests/spec_test_guest && cargo build --release`
//! 生成 wasm，再 `cargo test -p spacegate-plugin-wasm --test spec_compliance`。
//! 测试入口会通过 cargo metadata 自动定位 wasm 路径并按需在缺失时调用 cargo 触发构建。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;
use spacegate_plugin_wasm::config::WasmPluginShellConfig;
use spacegate_plugin_wasm::engine::shared_engine;
use spacegate_plugin_wasm::host_fn::register_all;
use spacegate_plugin_wasm::host_state::{ContextStage, HostState, HttpCallResult, PseudoHeaders, RequestContext};
use spacegate_plugin_wasm::vm::register_wasi_stubs;
use wasmtime::{Instance, Linker, Module, Store, TypedFunc};

const HTTP_CONTEXT_ID: u32 = 2;

// ─────────────────────────────────────────────────────────
// 定位 guest wasm；缺失则触发一次 `cargo build --release`
// ─────────────────────────────────────────────────────────

fn guest_manifest_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("spec_test_guest");
    p.push("Cargo.toml");
    p
}

/// 用 `cargo metadata` 拿独立 workspace 的 `target_directory`，再拼出 `wasm32-wasip1/release/spec_test_guest.wasm`。
fn guest_wasm_path() -> PathBuf {
    let manifest = guest_manifest_path();
    let out = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("cargo metadata: spawn");
    assert!(out.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse cargo metadata json");
    let target_dir = meta["target_directory"].as_str().expect("target_directory missing");
    PathBuf::from(target_dir).join("wasm32-wasip1").join("release").join("spec_test_guest.wasm")
}

fn ensure_guest_built() -> PathBuf {
    let wasm = guest_wasm_path();
    if !wasm.exists() {
        let manifest = guest_manifest_path();
        eprintln!("[spec_compliance] guest wasm not found at {wasm:?}; running `cargo build --release` for spec_test_guest");
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "--release", "--target", "wasm32-wasip1", "--manifest-path"])
            .arg(&manifest)
            .status()
            .expect("cargo build: spawn");
        assert!(status.success(), "spec_test_guest build failed (exit = {status:?})");
        assert!(wasm.exists(), "spec_test_guest.wasm still missing after build: {wasm:?}");
    }
    wasm
}

// ─────────────────────────────────────────────────────────
// 测试 harness：直接搭一个不走 Vm 的 store/linker/instance
// ─────────────────────────────────────────────────────────

struct GuestVm {
    store: Store<HostState>,
    instance: Instance,
}

impl GuestVm {
    fn new(wasm_bytes: &[u8], cfg: WasmPluginShellConfig, configuration: Vec<u8>) -> Self {
        let engine = shared_engine();
        let module = Module::new(engine, wasm_bytes).expect("Module::new");

        let mut host = HostState::new(Arc::new(cfg));
        host.configuration = configuration;
        // 预置一个 HTTP context，方便头部 / send_local_response 类场景。
        let ctx = RequestContext {
            parent_id: host.root_context_id,
            stage: ContextStage::RequestHeaders,
            request_pseudo: PseudoHeaders {
                method: "POST".into(),
                path: "/spec".into(),
                authority: "spec.local".into(),
                scheme: "http".into(),
            },
            request_headers: HeaderMap::new(),
            ..Default::default()
        };
        host.contexts.insert(HTTP_CONTEXT_ID, ctx);
        host.effective_context = HTTP_CONTEXT_ID;

        let mut store: Store<HostState> = Store::new(engine, host);
        let mut linker: Linker<HostState> = Linker::new(engine);
        // dispatch_tx 在本测试里不会被消费——保留 rx 不让通道关闭即可。
        let (dispatch_tx, _dispatch_rx) = tokio::sync::mpsc::unbounded_channel::<(u32, HttpCallResult)>();
        register_all(&mut linker, dispatch_tx).expect("register_all");
        register_wasi_stubs(&mut linker).expect("register_wasi_stubs");

        let instance = linker.instantiate(&mut store, &module).expect("instantiate");
        let mem = instance.get_memory(&mut store, "memory").expect("memory export");
        store.data_mut().memory = Some(mem);
        if let Ok(a) = instance.get_typed_func::<u32, u32>(&mut store, "proxy_on_memory_allocate") {
            store.data_mut().alloc = Some(a);
        } else if let Ok(a) = instance.get_typed_func::<u32, u32>(&mut store, "malloc") {
            store.data_mut().alloc = Some(a);
        } else {
            panic!("guest exports neither proxy_on_memory_allocate nor malloc");
        }

        // _initialize 优先（SDK 在 wasm32-wasip1 上默认导这个），回退 _start。
        if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
            init.call(&mut store, ()).expect("_initialize");
        } else if let Ok(start) = instance.get_typed_func::<(), ()>(&mut store, "_start") {
            start.call(&mut store, ()).expect("_start");
        }

        GuestVm { store, instance }
    }

    fn run_test(&mut self, scenario: u32) -> u32 {
        let f: TypedFunc<u32, u32> = self
            .instance
            .get_typed_func(&mut self.store, "__run_test")
            .expect("__run_test export");
        f.call(&mut self.store, scenario).expect("__run_test trap-free")
    }

    fn data(&self) -> &HostState {
        self.store.data()
    }
}

// ─────────────────────────────────────────────────────────
// 唯一一个 `#[test]` —— 跑完所有 scenario；隔离 shared/queue/metric 已通过 scenario 内独立 key 实现。
// ─────────────────────────────────────────────────────────

#[test]
fn proxy_wasm_spec_v0_2_1_compliance() {
    // 准备 wasm。
    let wasm_path = ensure_guest_built();
    let wasm_bytes = std::fs::read(&wasm_path).expect("read guest wasm");

    // 业务侧配置：plugin_name 用于 well-known property 校验；configuration 走 buffer 通道。
    let cfg = WasmPluginShellConfig {
        url: format!("file://{}", wasm_path.display()),
        plugin_config: serde_json::Value::Null,
        plugin_name: "spec-test-plugin".to_string(),
        plugin_root_id: "spec-test-root".to_string(),
        plugin_vm_id: "default".to_string(),
        clusters: HashMap::new(),
        ..Default::default()
    };
    let configuration = b"spec-test-config".to_vec();

    let mut vm = GuestVm::new(&wasm_bytes, cfg, configuration);

    // 依次跑场景；每个返回 0 视为通过。
    let scenarios: &[(u32, &str)] = &[
        (1, "shared_data CAS roundtrip"),
        (2, "shared_queue lifecycle"),
        (3, "metric counter increment-only"),
        (4, "metric gauge bidirectional + record"),
        (5, "user property set/get"),
        (6, "well-known plugin_name property"),
        (7, "get_log_level"),
        (8, "get_current_time_nanoseconds"),
        (9, "continue_stream(HTTP_REQUEST)"),
        (10, "close_stream(DOWNSTREAM) → Unimplemented"),
        (11, "set_effective_context(invalid) → BadArgument"),
        (12, "grpc_call → Unimplemented"),
        (13, "foreign_function → NotFound"),
        (14, "request `:method` pseudo header"),
        (15, "add/replace/remove header"),
        (16, "get_buffer(PluginConfiguration)"),
        (17, "send_local_response"),
        (18, "proxy_done without awaiting → NotFound"),
        (19, "log at all levels"),
        (20, "set_tick_period"),
    ];

    let mut failures: Vec<(u32, &str, u32)> = Vec::new();
    for &(id, name) in scenarios {
        // 每个 scenario 重置一次 effective_context（部分 scenario 会改它）
        vm.store.data_mut().effective_context = HTTP_CONTEXT_ID;
        let code = vm.run_test(id);
        if code != 0 {
            failures.push((id, name, code));
        }
    }

    assert!(failures.is_empty(), "spec compliance failures: {failures:?}");

    // 额外的 host 侧副作用断言：
    let st = vm.data();

    // (17) send_local_response 应写入 ctx.local_response
    let ctx = st.contexts.get(&HTTP_CONTEXT_ID).expect("http ctx present");
    let lr = ctx.local_response.as_ref().expect("local_response written by guest");
    assert_eq!(lr.status, 418, "local_response.status");
    assert_eq!(lr.body, Bytes::from_static(b"local body"), "local_response.body");
    let x_spec = lr
        .headers
        .get("x-spec")
        .expect("x-spec header present")
        .to_str()
        .unwrap_or("");
    assert_eq!(x_spec, "teapot");

    // (20) set_tick_period 应写入 HostState.tick_period_ms
    assert_eq!(st.tick_period_ms, Some(123), "tick_period_ms");

    // (5) user_properties 应记录我们设的 key
    // user_properties 的 key 是 path 用 \0 拼起来；spec_test_guest 设的是 vec!["spec","user_prop"]
    let want_key: Vec<u8> = b"spec\0user_prop".to_vec();
    let v = st.user_properties.get(&want_key).expect("user_prop stored under '\\0'-joined key");
    assert_eq!(v.as_slice(), b"hello");
}
