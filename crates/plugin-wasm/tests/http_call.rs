//! 端到端验证 `proxy_http_call` → `proxy_on_http_call_response` 链路：
//! 模式来自 [`sdk_examples_guest`] 的 `auth_random`，guest 在 `on_request_headers`
//! 发起一次外呼，host 通过 reqwest 真正打到一个本地 mock HTTP server，server
//! 返回一段固定字节；guest 的 `on_http_call_response` 根据第一个字节决定
//! `resume_http_request()` 放行 / `send_local_response(403)`。
//!
//! 这条测试是 `proxy_http_call` 的唯一覆盖路径——host fn 注册、token 分配、
//! reqwest spawn、UnboundedSender → drive_until_continue 状态机、effective_context
//! 切换、guest 通过 `get_http_call_response_body` 读 body —— 一次跑齐。

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response};
use hyper_util::rt::TokioIo;
use spacegate_kernel::backend_service::ArcHyperService;
use spacegate_kernel::helper_layers::function::Inner;
use spacegate_kernel::{SgBody, SgRequest, SgResponse};
use spacegate_plugin::{Plugin, PluginConfig, PluginInstanceId, PluginInstanceName};
use spacegate_plugin_wasm::config::WasmPluginShellConfig;
use spacegate_plugin_wasm::engine::shared_engine;
use spacegate_plugin_wasm::vm::Vm;
use spacegate_plugin_wasm::WasmPluginShell;
use tokio::net::TcpListener;
use wasmtime::Module;

// ─────────────────────────────────────────────────────────
// 共用：定位 sdk_examples_guest.wasm（与 sdk_examples.rs 相同；故意复制以保持测试独立）
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
    Arc::new(Module::new(shared_engine(), &bytes).expect("Module::new"))
}

// ─────────────────────────────────────────────────────────
// mock HTTP server：返回单字节 body，用于驱动 auth_random 判断
// ─────────────────────────────────────────────────────────

async fn start_mock_server(body_byte: u8) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            tokio::spawn(async move {
                let svc = service_fn(move |_req: HyperRequest<hyper::body::Incoming>| async move {
                    let body = Bytes::from(vec![body_byte]);
                    let resp = Response::builder().status(200).body(Full::new(body)).expect("build resp");
                    Ok::<_, Infallible>(resp)
                });
                let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), svc).await;
            });
        }
    });
    addr
}

async fn start_delayed_mock_server(body_byte: u8, delay: Duration) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            tokio::spawn(async move {
                let svc = service_fn(move |_req: HyperRequest<hyper::body::Incoming>| async move {
                    tokio::time::sleep(delay).await;
                    let body = Bytes::from(vec![body_byte]);
                    let resp = Response::builder().status(200).body(Full::new(body)).expect("build resp");
                    Ok::<_, Infallible>(resp)
                });
                let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), svc).await;
            });
        }
    });
    addr
}

// ─────────────────────────────────────────────────────────
// mock inner.call：guest 放行后会下沉到这里，echo body 即可
// ─────────────────────────────────────────────────────────

fn echo_inner() -> Inner {
    let svc = service_fn(|req: SgRequest| async move {
        let (_, body) = req.into_parts();
        let bytes = body.collect().await.map(|c| c.to_bytes()).unwrap_or_default();
        let mut resp = SgResponse::new(SgBody::full(bytes));
        *resp.status_mut() = http::StatusCode::OK;
        Ok::<_, Infallible>(resp)
    });
    Inner::new(ArcHyperService::new(svc))
}

async fn full_body(resp: SgResponse) -> (SgResponse, Bytes) {
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.map(|c| c.to_bytes()).unwrap_or_default();
    (SgResponse::from_parts(parts, SgBody::full(bytes.clone())), bytes)
}

async fn run(auth_byte: u8) -> (u16, Bytes) {
    let addr = start_mock_server(auth_byte).await;
    // 给 server 一个起跳的间隙；用 50ms 兜底（tokio 实际可即时 accept）。
    tokio::time::sleep(Duration::from_millis(20)).await;

    let module = load_module();
    let cfg = Arc::new(WasmPluginShellConfig {
        url: "file://sdk_examples_guest".into(),
        plugin_config: serde_json::json!({
            "mode": "auth_random",
            "auth_cluster": "auth",
            "auth_threshold": 128
        }),
        clusters: [("auth".to_string(), format!("http://{addr}"))].into_iter().collect(),
        ..Default::default()
    });
    let mut vm = Vm::new(&module, cfg).expect("Vm::new");

    let req = HyperRequest::builder()
        .method("POST")
        .uri("http://example.test/")
        .header("host", "example.test")
        .body(SgBody::full(Bytes::from_static(b"protected payload")))
        .expect("build req");

    let resp = vm.process(req, echo_inner()).await.expect("process");
    let (resp, body) = full_body(resp).await;
    (resp.status().as_u16(), body)
}

fn protected_request() -> SgRequest {
    protected_request_with_policy(None)
}

fn protected_request_with_policy(policy: Option<&str>) -> SgRequest {
    let mut builder = HyperRequest::builder().method("POST").uri("http://example.test/").header("host", "example.test");
    if let Some(policy) = policy {
        builder = builder.header("x-ratelimit-policy", policy);
    }
    builder.body(SgBody::full(Bytes::from_static(b"protected payload"))).expect("build req")
}

// ─────────────────────────────────────────────────────────
// auth byte < threshold → 放行；echo 回原 body
// ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_random_allow() {
    let (status, body) = run(50).await;
    assert_eq!(status, 200, "expected allow → echo");
    assert_eq!(body, Bytes::from_static(b"protected payload"));
}

// ─────────────────────────────────────────────────────────
// auth byte >= threshold → guest 短路 403
// ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_random_deny() {
    let (status, body) = run(200).await;
    assert_eq!(status, 403, "expected deny → 403");
    assert_eq!(body, Bytes::from_static(b"forbidden"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn vm_pool_runs_slow_http_calls_concurrently() {
    let wasm = ensure_guest_built();
    let addr = start_delayed_mock_server(50, Duration::from_millis(450)).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    let shell = WasmPluginShell::create(PluginConfig {
        id: PluginInstanceId {
            code: "wasm".into(),
            name: PluginInstanceName::named("vm-pool-test"),
        },
        spec: serde_json::json!({
            "url": format!("file://{}", wasm.display()),
            "plugin_config": {
                "mode": "auth_random",
                "auth_cluster": "auth",
                "auth_threshold": 128
            },
            "clusters": {
                "auth": format!("http://{addr}")
            },
            "vm_pool_size": 2
        }),
    })
    .expect("create wasm shell");

    let started = Instant::now();
    let (resp1, resp2) = tokio::join!(shell.call(protected_request(), echo_inner()), shell.call(protected_request(), echo_inner()));
    let elapsed = started.elapsed();

    let (resp1, body1) = full_body(resp1.expect("resp1")).await;
    let (resp2, body2) = full_body(resp2.expect("resp2")).await;
    assert_eq!(resp1.status(), http::StatusCode::OK);
    assert_eq!(resp2.status(), http::StatusCode::OK);
    assert_eq!(body1, Bytes::from_static(b"protected payload"));
    assert_eq!(body2, Bytes::from_static(b"protected payload"));
    assert!(
        elapsed < Duration::from_millis(800),
        "expected two 450ms dispatches to overlap with vm_pool_size=2, elapsed={elapsed:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wait_policy_uses_separate_vm_pool() {
    let wasm = ensure_guest_built();
    let addr = start_delayed_mock_server(50, Duration::from_millis(450)).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    let shell = WasmPluginShell::create(PluginConfig {
        id: PluginInstanceId {
            code: "wasm".into(),
            name: PluginInstanceName::named("wait-vm-pool-test"),
        },
        spec: serde_json::json!({
            "url": format!("file://{}", wasm.display()),
            "plugin_config": {
                "mode": "auth_random",
                "auth_cluster": "auth",
                "auth_threshold": 128
            },
            "clusters": {
                "auth": format!("http://{addr}")
            },
            "vm_pool_size": 1,
            "wait_vm_pool_size": 1
        }),
    })
    .expect("create wasm shell");

    let started = Instant::now();
    let (wait_resp, normal_resp) = tokio::join!(shell.call(protected_request_with_policy(Some("wait")), echo_inner()), async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        shell.call(protected_request(), echo_inner()).await
    });
    let elapsed = started.elapsed();

    let (wait_resp, wait_body) = full_body(wait_resp.expect("wait resp")).await;
    let (normal_resp, normal_body) = full_body(normal_resp.expect("normal resp")).await;
    assert_eq!(wait_resp.status(), http::StatusCode::OK);
    assert_eq!(normal_resp.status(), http::StatusCode::OK);
    assert_eq!(wait_body, Bytes::from_static(b"protected payload"));
    assert_eq!(normal_body, Bytes::from_static(b"protected payload"));
    assert!(
        elapsed < Duration::from_millis(800),
        "expected wait traffic to use wait_vm_pool and not block normal pool, elapsed={elapsed:?}"
    );
}
