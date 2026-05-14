//! 用 `proxy-wasm-rust-sdk` 仓库 `examples/` 的 4 个范例对应行为构造
//! [`sdk_examples_guest`] 单 wasm 多模式，逐个跑完整 [`Vm::process`] 链路。
//!
//! 这个测试是真正的端到端：plugin configuration → on_configure → 请求进入插件 →
//! on_request_headers / on_request_body → inner.call（我们 mock 出来的 hyper 服务）→
//! on_response_headers → 最终响应 → on_log。它直接证明：
//!
//! - SDK 标准范例的 host fn 调用面我们 host 全部正确实现；
//! - body 改写、本地响应短路、required header 拦截 这些跨阶段的协同没问题。
//!
//! 本文件 **不** 覆盖 `auth_random`（需要 mock HTTP server 走 reqwest），那部分在
//! [`tests/http_call.rs`] 单独测。

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::service::service_fn;
use spacegate_kernel::backend_service::ArcHyperService;
use spacegate_kernel::helper_layers::function::Inner;
use hyper::Request as HyperRequest;
use spacegate_kernel::{SgBody, SgRequest, SgResponse};
use spacegate_plugin_wasm::config::WasmPluginShellConfig;
use spacegate_plugin_wasm::engine::shared_engine;
use spacegate_plugin_wasm::vm::Vm;
use wasmtime::Module;

// ─────────────────────────────────────────────────────────
// 公共：定位/构建 sdk_examples_guest.wasm
// ─────────────────────────────────────────────────────────

fn guest_manifest_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("sdk_examples_guest");
    p.push("Cargo.toml");
    p
}

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
    PathBuf::from(target_dir).join("wasm32-wasip1").join("release").join("sdk_examples_guest.wasm")
}

fn ensure_guest_built() -> PathBuf {
    let wasm = guest_wasm_path();
    if !wasm.exists() {
        eprintln!("[sdk_examples] building sdk_examples_guest …");
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "--release", "--target", "wasm32-wasip1", "--manifest-path"])
            .arg(guest_manifest_path())
            .status()
            .expect("cargo build: spawn");
        assert!(status.success(), "sdk_examples_guest build failed");
        assert!(wasm.exists(), "wasm still missing after build: {wasm:?}");
    }
    wasm
}

fn load_module() -> Arc<Module> {
    let path = ensure_guest_built();
    let bytes = std::fs::read(&path).expect("read wasm");
    let module = Module::new(shared_engine(), &bytes).expect("Module::new");
    Arc::new(module)
}

// ─────────────────────────────────────────────────────────
// mock `Inner`：把请求 body 原样回显，并复制 `x-echo-*` 头
// ─────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct CaptureState {
    /// inner.call 真正收到的请求体（guest 修改后下沉的内容）
    inbound_body: Arc<tokio::sync::Mutex<Option<Bytes>>>,
    /// inner.call 实际是否被调到（验证 send_local_response 的短路）
    invoked: Arc<std::sync::atomic::AtomicBool>,
}

fn make_inner(state: CaptureState) -> Inner {
    let svc = service_fn(move |req: SgRequest| {
        let state = state.clone();
        async move {
            state.invoked.store(true, std::sync::atomic::Ordering::SeqCst);
            let (parts, body) = req.into_parts();
            let bytes = body.collect().await.map(|c| c.to_bytes()).unwrap_or_default();
            *state.inbound_body.lock().await = Some(bytes.clone());
            let mut resp = SgResponse::new(SgBody::full(bytes));
            for (k, v) in parts.headers.iter() {
                if k.as_str().starts_with("x-echo-") {
                    resp.headers_mut().insert(k, v.clone());
                }
            }
            Ok::<_, Infallible>(resp)
        }
    });
    Inner::new(ArcHyperService::new(svc))
}

fn make_cfg(spec: serde_json::Value) -> Arc<WasmPluginShellConfig> {
    Arc::new(WasmPluginShellConfig {
        url: "file://sdk_examples_guest".into(),
        plugin_config: spec,
        plugin_name: "sdk-examples-test".into(),
        plugin_root_id: "sdk-examples-root".into(),
        plugin_vm_id: "default".into(),
        ..Default::default()
    })
}

async fn full_body(resp: SgResponse) -> (SgResponse, Bytes) {
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.map(|c| c.to_bytes()).unwrap_or_default();
    (SgResponse::from_parts(parts, SgBody::full(bytes.clone())), bytes)
}

// ─────────────────────────────────────────────────────────
// 1. http_headers：/hello → 本地 200 + Hello/Powered-By；其余 → 走 inner，加 x-sdk-headers 响应头
// ─────────────────────────────────────────────────────────

#[tokio::test]
async fn sdk_example_http_headers_hello() {
    let module = load_module();
    let cfg = make_cfg(serde_json::json!({"mode": "headers"}));
    let mut vm = Vm::new(&module, cfg).expect("Vm::new");

    let req = HyperRequest::builder()
        .method("GET")
        .uri("http://example.test/hello")
        .header("host", "example.test")
        .body(SgBody::empty())
        .expect("build req");
    let captured = CaptureState::default();
    let inner = make_inner(captured.clone());
    let resp = vm.process(req, inner).await.expect("process");
    let (resp, body) = full_body(resp).await;

    assert_eq!(resp.status(), 200);
    assert_eq!(body, Bytes::from_static(b"Hello, World!\n"));
    assert_eq!(resp.headers().get("hello").and_then(|v| v.to_str().ok()), Some("world"));
    assert_eq!(
        resp.headers().get("powered-by").and_then(|v| v.to_str().ok()),
        Some("proxy-wasm")
    );
    assert!(
        !captured.invoked.load(std::sync::atomic::Ordering::SeqCst),
        "inner.call must NOT be invoked for local response"
    );
}

#[tokio::test]
async fn sdk_example_http_headers_passthrough() {
    let module = load_module();
    let cfg = make_cfg(serde_json::json!({"mode": "headers"}));
    let mut vm = Vm::new(&module, cfg).expect("Vm::new");

    let req = HyperRequest::builder()
        .method("GET")
        .uri("http://example.test/world")
        .header("host", "example.test")
        .header("x-echo-foo", "bar")
        .body(SgBody::empty())
        .expect("build req");
    let captured = CaptureState::default();
    let resp = vm.process(req, make_inner(captured.clone())).await.expect("process");
    let (resp, _body) = full_body(resp).await;

    assert_eq!(resp.status(), 200);
    assert!(captured.invoked.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        resp.headers().get("x-sdk-headers").and_then(|v| v.to_str().ok()),
        Some("seen"),
        "on_response_headers should inject x-sdk-headers"
    );
    // echo header 应该原路回来
    assert_eq!(
        resp.headers().get("x-echo-foo").and_then(|v| v.to_str().ok()),
        Some("bar")
    );
}

// ─────────────────────────────────────────────────────────
// 2. http_body：on_request_body 把 body 反转后下沉给 inner.call
// ─────────────────────────────────────────────────────────

#[tokio::test]
async fn sdk_example_http_body_reverses_request_body() {
    let module = load_module();
    let cfg = make_cfg(serde_json::json!({"mode": "body"}));
    let mut vm = Vm::new(&module, cfg).expect("Vm::new");

    let req = HyperRequest::builder()
        .method("POST")
        .uri("http://example.test/reverse")
        .header("host", "example.test")
        .body(SgBody::full(Bytes::from_static(b"abc-123")))
        .expect("build req");
    let captured = CaptureState::default();
    let resp = vm.process(req, make_inner(captured.clone())).await.expect("process");
    let (resp, body) = full_body(resp).await;

    assert_eq!(resp.status(), 200);
    // Inner 收到的应是反转后的字节，echo 回来后响应体也是它。
    assert_eq!(
        captured.inbound_body.lock().await.clone().expect("body captured"),
        Bytes::from_static(b"321-cba")
    );
    assert_eq!(body, Bytes::from_static(b"321-cba"));
}

// ─────────────────────────────────────────────────────────
// 3. http_config：缺失 x-token → 本地 403；带上 → 放行
// ─────────────────────────────────────────────────────────

#[tokio::test]
async fn sdk_example_http_config_missing_header_rejected() {
    let module = load_module();
    let cfg = make_cfg(serde_json::json!({"mode": "config", "required_header": "x-token"}));
    let mut vm = Vm::new(&module, cfg).expect("Vm::new");

    let req = HyperRequest::builder()
        .method("GET")
        .uri("http://example.test/")
        .header("host", "example.test")
        .body(SgBody::empty())
        .expect("build req");
    let captured = CaptureState::default();
    let resp = vm.process(req, make_inner(captured.clone())).await.expect("process");
    let (resp, body) = full_body(resp).await;

    assert_eq!(resp.status(), 403);
    assert_eq!(body, Bytes::from_static(b"missing required header"));
    assert!(!captured.invoked.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn sdk_example_http_config_present_header_passthrough() {
    let module = load_module();
    let cfg = make_cfg(serde_json::json!({"mode": "config", "required_header": "x-token"}));
    let mut vm = Vm::new(&module, cfg).expect("Vm::new");

    let req = HyperRequest::builder()
        .method("GET")
        .uri("http://example.test/")
        .header("host", "example.test")
        .header("x-token", "abc")
        .body(SgBody::full(Bytes::from_static(b"hello")))
        .expect("build req");
    let captured = CaptureState::default();
    let resp = vm.process(req, make_inner(captured.clone())).await.expect("process");
    let (resp, body) = full_body(resp).await;

    assert_eq!(resp.status(), 200);
    assert_eq!(body, Bytes::from_static(b"hello"));
    assert!(captured.invoked.load(std::sync::atomic::Ordering::SeqCst));
}
