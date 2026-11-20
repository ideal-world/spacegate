//! 使用真实TCP和HTTP/2 stream验证完整请求体之后的等待取消，不以服务端abort代替。
use std::{convert::Infallible, net::SocketAddr, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};

use http_body_util::BodyExt;
use hyper::{Request, Response, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use spacegate_kernel::{ArcHyperService, SgBody, service::{Http, TcpService}};
use tokio::{io::AsyncWriteExt, net::{TcpListener, TcpStream}, sync::{Notify, mpsc}, task::JoinHandle, time::timeout};

const BOUND: Duration = Duration::from_secs(2);

struct WaitGuard { dropped: Arc<AtomicBool> }
impl Drop for WaitGuard {
    fn drop(&mut self) { self.dropped.store(true, Ordering::SeqCst); }
}

struct Probe {
    address: SocketAddr,
    entered: mpsc::UnboundedReceiver<String>,
    dropped: Arc<Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>>,
    release: Arc<Notify>,
    accept_task: JoinHandle<()>,
}
impl Drop for Probe {
    fn drop(&mut self) { self.release.notify_waiters(); self.accept_task.abort(); }
}
impl Probe {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (entered_tx, entered) = mpsc::unbounded_channel();
        let dropped = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let release = Arc::new(Notify::new());
        let service = ArcHyperService::new(service_fn({
            let dropped = dropped.clone(); let release = release.clone();
            move |req: Request<SgBody>| {
                let dropped = dropped.clone(); let release = release.clone(); let entered_tx = entered_tx.clone();
                async move {
                    let id = req.uri().path().to_string();
                    let body = req.into_body().collect().await.unwrap().to_bytes();
                    assert_eq!(body.as_ref(), b"ready");
                    let marker = Arc::new(AtomicBool::new(false));
                    dropped.lock().unwrap().insert(id.clone(), marker.clone());
                    let _guard = WaitGuard { dropped: marker };
                    entered_tx.send(id).unwrap();
                    release.notified().await;
                    Ok::<_, Infallible>(Response::new(SgBody::full("released")))
                }
            }
        }));
        let http = Arc::new(Http::new(service));
        let accept_task = tokio::spawn(async move {
            while let Ok((stream, peer)) = listener.accept().await {
                let http = http.clone();
                tokio::spawn(async move { let _ = http.handle(stream, peer).await; });
            }
        });
        Self { address, entered, dropped, release, accept_task }
    }
    async fn enter(&mut self, path: &str) {
        assert_eq!(timeout(BOUND, self.entered.recv()).await.unwrap().as_deref(), Some(path));
    }
    fn was_dropped(&self, path: &str) -> bool {
        self.dropped.lock().unwrap().get(path).unwrap().load(Ordering::SeqCst)
    }
    async fn expect_drop(&self, path: &str) {
        timeout(BOUND, async { while !self.was_dropped(path) { tokio::task::yield_now().await; } }).await.expect("network cancellation must drop waiting service future");
    }
    async fn h1(&self, path: &str) -> TcpStream {
        let mut stream = TcpStream::connect(self.address).await.unwrap();
        stream.write_all(format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nready").as_bytes()).await.unwrap();
        stream
    }
}

#[tokio::test]
async fn test_http1_disconnect_after_full_body_drops_wait() {
    let mut probe = Probe::start().await;
    let stream = probe.h1("/disconnect").await;
    probe.enter("/disconnect").await;
    drop(stream);
    probe.expect_drop("/disconnect").await;
}

#[tokio::test]
async fn test_http1_rst_drops_wait() {
    let mut probe = Probe::start().await;
    let stream = probe.h1("/reset").await;
    probe.enter("/reset").await;
    let stream = stream.into_std().unwrap();
    socket2::SockRef::from(&stream).set_linger(Some(Duration::ZERO)).unwrap();
    drop(stream);
    probe.expect_drop("/reset").await;
}

#[tokio::test]
async fn test_http1_half_close_preserves_existing_transport_cancel_policy() {
    let mut probe = Probe::start().await;
    let mut stream = probe.h1("/half-close").await;
    probe.enter("/half-close").await;
    // 用户确认保留现有策略：完整上传后的FIN也视为传输结束，不新增半关闭支持。
    stream.shutdown().await.unwrap();
    probe.expect_drop("/half-close").await;
}

#[tokio::test]
async fn test_body_eof_alone_does_not_cancel_wait() {
    let mut probe = Probe::start().await;
    let _stream = probe.h1("/body-eof").await;
    probe.enter("/body-eof").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!probe.was_dropped("/body-eof"));
    probe.release.notify_waiters();
    probe.expect_drop("/body-eof").await;
}

#[tokio::test]
async fn test_stopping_accept_does_not_cancel_waiting_request() {
    let mut probe = Probe::start().await;
    let _stream = probe.h1("/stop-accept").await;
    probe.enter("/stop-accept").await;
    probe.accept_task.abort();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!probe.was_dropped("/stop-accept"), "queue runtime must explicitly drain existing waiters");
    probe.release.notify_waiters();
    probe.expect_drop("/stop-accept").await;
}

#[tokio::test]
async fn test_h2_reset_only_cancels_target_stream() {
    let mut probe = Probe::start().await;
    let stream = TcpStream::connect(probe.address).await.unwrap();
    let (mut client, connection) = hyper::client::conn::http2::handshake::<_, _, SgBody>(TokioExecutor::new(), TokioIo::new(stream)).await.unwrap();
    let connection_task = tokio::spawn(connection);
    let first = Request::post("http://localhost/reset").body(SgBody::full("ready")).unwrap();
    let request_task = tokio::spawn(client.send_request(first));
    probe.enter("/reset").await;
    let other = Request::post("http://localhost/keep").body(SgBody::full("ready")).unwrap();
    let other_task = tokio::spawn(client.send_request(other));
    probe.enter("/keep").await;
    // 丢弃客户端的单次响应Future，Hyper会向真实连接发送RST_STREAM(CANCEL)。
    request_task.abort();
    let _ = request_task.await;
    probe.expect_drop("/reset").await;
    assert!(!probe.was_dropped("/keep"), "reset must not cancel another H2 stream");
    probe.release.notify_waiters();
    let response = timeout(BOUND, other_task).await.unwrap().unwrap().unwrap();
    assert_eq!(response.into_body().collect().await.unwrap().to_bytes(), "released");
    connection_task.abort();
}
