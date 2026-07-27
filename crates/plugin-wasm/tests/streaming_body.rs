//! 验证 Proxy-Wasm host 对 SSE 响应执行逐 chunk body hook，并单独发送一次 EOF 回调。

use std::convert::Infallible;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::body::{Body, Frame};
use hyper::service::service_fn;
use spacegate_kernel::backend_service::ArcHyperService;
use spacegate_kernel::helper_layers::function::Inner;
use spacegate_kernel::{SgBody, SgRequest, SgResponse};
use spacegate_plugin_wasm::config::WasmPluginShellConfig;
use spacegate_plugin_wasm::engine::shared_engine;
use spacegate_plugin_wasm::shared::{shared_data_get, shared_data_set};
use spacegate_plugin_wasm::streaming_body::spawn_response_stream;
use spacegate_plugin_wasm::vm::{Vm, VmProcessResult};
use tokio::sync::Notify;
use wasmtime::Module;

/// 返回独立 guest crate 的 manifest 路径。
fn guest_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/streaming_body_guest/Cargo.toml")
}

/// 通过 guest workspace metadata 定位 wasm32-wasip1 release 产物。
fn guest_wasm_path() -> PathBuf {
    let manifest = guest_manifest_path();
    let output = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("cargo metadata: spawn");
    assert!(output.status.success(), "cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse cargo metadata");
    let target_dir = metadata["target_directory"].as_str().expect("target_directory");
    PathBuf::from(target_dir).join("wasm32-wasip1/release/streaming_body_guest.wasm")
}

/// 按需构建 SSE 测试 guest，并返回 wasm 文件路径。
fn ensure_guest_built() -> PathBuf {
    let wasm = guest_wasm_path();
    if !wasm.exists() {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "--release", "--target", "wasm32-wasip1", "--manifest-path"])
            .arg(guest_manifest_path())
            .status()
            .expect("cargo build: spawn");
        assert!(status.success(), "streaming_body_guest build failed");
        assert!(wasm.exists(), "guest wasm missing after build: {wasm:?}");
    }
    wasm
}

/// 创建配置了独立 shared-data key 的测试 VM。
fn make_vm(state_key: &str) -> Vm {
    make_vm_with_mode(state_key, "rewrite")
}

/// 创建指定 Hai 语义模式的测试 VM。
fn make_vm_with_mode(state_key: &str, mode: &str) -> Vm {
    let wasm = std::fs::read(ensure_guest_built()).expect("read guest wasm");
    let module = Module::new(shared_engine(), wasm).expect("compile guest wasm");
    let config = Arc::new(WasmPluginShellConfig {
        url: "file://streaming_body_guest".to_string(),
        plugin_config: serde_json::json!({"state_key": state_key, "mode": mode}),
        plugin_name: "streaming-body-test".to_string(),
        plugin_root_id: "streaming-body-root".to_string(),
        plugin_vm_id: "streaming-body-vm".to_string(),
        ..Default::default()
    });
    Vm::new(&module, config).expect("create VM")
}

/// 控制第二个 SSE chunk 的释放时机。
#[derive(Clone)]
struct SseControl {
    second_chunk: Arc<Notify>,
}

impl SseControl {
    /// 允许 mock upstream 产生第二个 SSE chunk 和 EOF。
    fn release_second_chunk(&self) {
        self.second_chunk.notify_one();
    }
}

/// 先产生首个 chunk，再等待通知产生第二个 chunk 的测试 body。
struct ControlledSseBody {
    /// 当前已发送到哪一个预设响应阶段。
    stage: u8,
    /// 等待测试侧允许第二个 SSE chunk 的通知 future。
    wait_for_second: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
    /// 通知前后分别发送的固定 SSE payload。
    chunks: [Bytes; 2],
}

impl ControlledSseBody {
    /// 创建 body 与控制器，使测试可在断言首段后再释放尾段。
    fn new(first: Bytes, second: Bytes) -> (Self, SseControl) {
        let second_chunk = Arc::new(Notify::new());
        let wait_notify = second_chunk.clone();
        let wait_for_second = Box::pin(async move { wait_notify.notified().await });
        (
            Self {
                stage: 0,
                wait_for_second: Mutex::new(wait_for_second),
                chunks: [first, second],
            },
            SseControl { second_chunk },
        )
    }
}

impl Body for ControlledSseBody {
    type Data = Bytes;
    type Error = Infallible;

    /// 按首段、通知等待、尾段、EOF 的顺序驱动 mock upstream。
    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.stage {
            0 => {
                self.stage = 1;
                Poll::Ready(Some(Ok(Frame::data(self.chunks[0].clone()))))
            }
            1 => {
                let ready = {
                    let wait = self.wait_for_second.get_mut().expect("notification mutex poisoned");
                    wait.as_mut().poll(cx).is_ready()
                };
                if ready {
                    self.stage = 2;
                    Poll::Ready(Some(Ok(Frame::data(self.chunks[1].clone()))))
                } else {
                    Poll::Pending
                }
            }
            _ => Poll::Ready(None),
        }
    }
}

/// 创建返回受控 SSE body 的 mock upstream。
fn make_streaming_inner() -> (Inner, SseControl) {
    make_streaming_inner_with_chunks(Bytes::from_static(b"data: first\n\n"), Bytes::from_static(b"data: second\n\n"))
}

/// 创建携带指定两段 SSE payload 的 mock upstream。
fn make_streaming_inner_with_chunks(first: Bytes, second: Bytes) -> (Inner, SseControl) {
    let (body, control) = ControlledSseBody::new(first, second);
    let body = Arc::new(Mutex::new(Some(body)));
    let service = service_fn(move |_request: SgRequest| {
        let body = body.lock().expect("body mutex poisoned").take().expect("upstream called once");
        async move {
            Ok::<_, Infallible>(
                hyper::Response::builder().header("content-type", "text/event-stream").body(SgBody::new(body)).expect("SSE response"),
            )
        }
    });
    (Inner::new(ArcHyperService::new(service)), control)
}

/// 创建通过 response-body hook 的基础请求。
fn request() -> SgRequest {
    hyper::Request::builder().method("GET").uri("http://example.test/mcp").body(SgBody::empty()).expect("request")
}

/// 读取 guest 发布的回调状态。
fn callback_state(state_key: &str) -> String {
    let (value, _cas) = shared_data_get(state_key.as_bytes()).expect("guest callback state");
    String::from_utf8(value).expect("UTF-8 callback state")
}

/// 完成请求/响应头阶段，并为 SSE 结果启动逐 chunk worker。
async fn start_streaming_response(vm: Vm, inner: Inner) -> SgResponse {
    let vm = Arc::new(tokio::sync::Mutex::new(vm));
    start_streaming_response_on_vm(vm, inner).await
}

/// 在共享的单 VM 上启动一个 SSE response，验证 chunk 间可复用同一 slot。
async fn start_streaming_response_on_vm(vm: Arc<tokio::sync::Mutex<Vm>>, inner: Inner) -> SgResponse {
    let result = {
        let mut guard = vm.lock().await;
        guard.process(request(), inner).await.expect("prepare SSE response")
    };
    match result {
        VmProcessResult::Streaming(prepared) => spawn_response_stream(vm, prepared, || {}),
        VmProcessResult::Complete(_) => panic!("SSE response must use streaming body path"),
    }
}

/// 首个 SSE chunk 必须在 upstream 产生第二个 chunk 之前完成 guest 改写并抵达客户端。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wasm_response_body_hook_receives_first_sse_chunk_before_upstream_eof() {
    let state_key = "streaming-body.first-before-eof";
    let _ = shared_data_set(state_key.as_bytes(), b"callbacks=0;eof=0", 0);
    let vm = make_vm(state_key);
    let (inner, control) = make_streaming_inner();

    let mut response = tokio::time::timeout(Duration::from_secs(2), start_streaming_response(vm, inner))
        .await
        .expect("Vm::process must return before upstream EOF");
    let first = tokio::time::timeout(Duration::from_secs(2), response.body_mut().frame())
        .await
        .expect("first SSE chunk must arrive before upstream EOF")
        .expect("first body frame")
        .expect("first frame result")
        .into_data()
        .expect("first frame data");

    assert_eq!(first, Bytes::from_static(b"data: filtered:first\n\n"));
    assert_eq!(callback_state(state_key), "callbacks=1;eof=0");

    control.release_second_chunk();
    let _ = response.into_body().collect().await.expect("finish SSE body");
}

/// 两个数据 chunk 后必须恰好触发一次独立的 EOF body 回调。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wasm_response_body_hook_receives_eof_once() {
    let state_key = "streaming-body.eof-once";
    let _ = shared_data_set(state_key.as_bytes(), b"callbacks=0;eof=0", 0);
    let vm = make_vm(state_key);
    let (inner, control) = make_streaming_inner();
    control.release_second_chunk();

    let response = start_streaming_response(vm, inner).await;
    let body = response.into_body().collect().await.expect("collect rewritten SSE").to_bytes();

    assert_eq!(body, Bytes::from_static(b"data: filtered:first\n\ndata: filtered:second\n\n"));
    assert_eq!(callback_state(state_key), "callbacks=3;eof=1");
}

/// Hai 风格 usage 必须跨两个 SSE chunk 累计，并只在 EOF 时发布最终值。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wasm_response_body_hook_accumulates_usage_until_eof() {
    let state_key = "streaming-body.usage";
    let _ = shared_data_set(state_key.as_bytes(), b"callbacks=0;eof=0", 0);
    let vm = make_vm_with_mode(state_key, "usage");
    let (inner, control) = make_streaming_inner_with_chunks(Bytes::from_static(b"data: usage=2\n\n"), Bytes::from_static(b"data: usage=3\n\n"));
    let mut response = start_streaming_response(vm, inner).await;

    let _ = response.body_mut().frame().await.expect("first frame").expect("first frame result");
    assert_eq!(callback_state(state_key), "callbacks=1;eof=0");
    control.release_second_chunk();
    let _ = response.into_body().collect().await.expect("finish response");

    assert_eq!(callback_state(state_key), "callbacks=3;eof=1;usage=5");
}

/// Hai 风格敏感输出过滤可删除单个 SSE chunk，随后仍能完成后续 chunk 与 EOF 清理。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wasm_response_body_hook_removes_blocked_chunk_and_finishes_cleanup() {
    let state_key = "streaming-body.block";
    let _ = shared_data_set(state_key.as_bytes(), b"callbacks=0;eof=0;blocked=0", 0);
    let vm = make_vm_with_mode(state_key, "block");
    let (inner, control) = make_streaming_inner_with_chunks(Bytes::from_static(b"data: blocked\n\n"), Bytes::from_static(b"data: allowed\n\n"));
    let mut response = start_streaming_response(vm, inner).await;

    let first = response.body_mut().frame().await.expect("blocked frame").expect("blocked frame result").into_data().expect("blocked data");
    assert!(first.is_empty());
    control.release_second_chunk();
    let rest = response.into_body().collect().await.expect("finish response").to_bytes();

    assert_eq!(rest, Bytes::from_static(b"data: filtered:allowed\n\n"));
    assert_eq!(callback_state(state_key), "callbacks=3;eof=1;blocked=1");
}

/// 单 VM 在首个 SSE stream 等待下一块时，仍能为第二个 stream 执行独立的 chunk callback。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streaming_vm_pool_size_one_allows_second_request_between_chunks() {
    let vm = Arc::new(tokio::sync::Mutex::new(make_vm("streaming-body.concurrent")));
    let (first_inner, first_control) = make_streaming_inner();
    let mut first = start_streaming_response_on_vm(vm.clone(), first_inner).await;
    let _ = first.body_mut().frame().await.expect("first stream chunk").expect("first stream frame");

    let (second_inner, second_control) = make_streaming_inner();
    let mut second = tokio::time::timeout(Duration::from_secs(2), start_streaming_response_on_vm(vm, second_inner))
        .await
        .expect("second stream must prepare while first waits");
    let second_chunk = second.body_mut().frame().await.expect("second stream chunk").expect("second stream frame").into_data().expect("second data");
    assert_eq!(second_chunk, Bytes::from_static(b"data: filtered:first\n\n"));

    first_control.release_second_chunk();
    second_control.release_second_chunk();
    drop(first);
    drop(second);
}

/// 客户端在 EOF 前断开时，stream worker 必须清理 context，使同一 VM 可立即服务下一请求。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_client_stream_releases_vm_for_next_request() {
    let vm = Arc::new(tokio::sync::Mutex::new(make_vm("streaming-body.disconnect")));
    let (abandoned_inner, _abandoned_control) = make_streaming_inner();
    let abandoned = start_streaming_response_on_vm(vm.clone(), abandoned_inner).await;
    drop(abandoned);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (next_inner, next_control) = make_streaming_inner();
    let mut next = tokio::time::timeout(Duration::from_secs(2), start_streaming_response_on_vm(vm, next_inner))
        .await
        .expect("VM must accept a request after client disconnect");
    let chunk = next.body_mut().frame().await.expect("next stream chunk").expect("next stream frame").into_data().expect("next data");
    assert_eq!(chunk, Bytes::from_static(b"data: filtered:first\n\n"));
    next_control.release_second_chunk();
    drop(next);
}
