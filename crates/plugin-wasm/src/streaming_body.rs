//! Proxy-Wasm SSE 响应流驱动器。
//!
//! 上游 body 在独立 Tokio task 中逐 frame 消费；每个 guest callback 只短暂锁定 VM，
//! 修改后的数据通过有界 channel 立即下发，并在客户端取消时完成 context 清理。

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::body::{Body, Frame};
use spacegate_kernel::{BoxError, SgBody, SgResponse};
use tokio::sync::{mpsc, Mutex};

use crate::vm::Vm;

/// 已完成响应头回调、等待异步驱动 body 的 SSE 响应。
#[derive(Debug)]
pub struct PreparedWasmResponseStream {
    /// 经 guest 响应头 hook 修改后的 HTTP response parts。
    pub parts: http::response::Parts,
    /// 尚未消费的 upstream streaming body。
    pub upstream_body: SgBody,
    /// 本次请求对应的 Proxy-Wasm HTTP context ID。
    pub http_context_id: u32,
}

/// 从 Tokio channel 接收经 guest 修改后的响应 frames。
struct ChannelBody {
    receiver: mpsc::Receiver<Result<Frame<Bytes>, BoxError>>,
}

impl Body for ChannelBody {
    type Data = Bytes;
    type Error = BoxError;

    /// 将 channel 中的下一个 frame 暴露给 Hyper body consumer。
    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        self.receiver.poll_recv(cx)
    }
}

/// 启动 SSE body worker，并立即返回客户端可轮询的响应。
///
/// `on_finish` 持有 shell 的 inflight guard；无论正常 EOF、upstream 错误还是客户端取消，
/// worker 退出时都会释放该 guard。
pub fn spawn_response_stream<F>(vm: Arc<Mutex<Vm>>, prepared: PreparedWasmResponseStream, on_finish: F) -> SgResponse
where
    F: FnOnce() + Send + 'static,
{
    let PreparedWasmResponseStream {
        parts,
        upstream_body,
        http_context_id,
    } = prepared;
    let (sender, receiver) = mpsc::channel(1);
    tokio::spawn(async move {
        drive_response_stream(vm, upstream_body, http_context_id, sender).await;
        on_finish();
    });
    SgResponse::from_parts(parts, SgBody::new(ChannelBody { receiver }))
}

/// 逐 frame 驱动 guest hook，并在所有退出路径清理 HTTP context。
async fn drive_response_stream(vm: Arc<Mutex<Vm>>, mut upstream: SgBody, http_context_id: u32, sender: mpsc::Sender<Result<Frame<Bytes>, BoxError>>) {
    loop {
        let frame = tokio::select! {
            _ = sender.closed() => {
                finish_stream_context(&vm, http_context_id).await;
                return;
            }
            frame = upstream.frame() => frame,
        };

        match frame {
            Some(Ok(frame)) => match frame.into_data() {
                Ok(bytes) => {
                    let processed = {
                        let mut vm = vm.lock().await;
                        vm.process_response_chunk(http_context_id, bytes, false).await
                    };
                    match processed {
                        Ok(processed) => {
                            if sender.send(Ok(Frame::data(processed.bytes))).await.is_err() {
                                finish_stream_context(&vm, http_context_id).await;
                                return;
                            }
                        }
                        Err(error) => {
                            let _ = sender.send(Err(Box::new(error))).await;
                            finish_stream_context(&vm, http_context_id).await;
                            return;
                        }
                    }
                }
                Err(frame) => {
                    let Ok(trailers) = frame.into_trailers() else {
                        continue;
                    };
                    let processed = {
                        let mut vm = vm.lock().await;
                        vm.process_response_trailers(http_context_id, trailers).await
                    };
                    match processed {
                        Ok(trailers) => {
                            if sender.send(Ok(Frame::trailers(trailers))).await.is_err() {
                                finish_stream_context(&vm, http_context_id).await;
                                return;
                            }
                        }
                        Err(error) => {
                            let _ = sender.send(Err(Box::new(error))).await;
                            finish_stream_context(&vm, http_context_id).await;
                            return;
                        }
                    }
                }
            },
            Some(Err(error)) => {
                let _ = sender.send(Err(error)).await;
                finish_stream_context(&vm, http_context_id).await;
                return;
            }
            None => {
                let terminal = {
                    let mut vm = vm.lock().await;
                    vm.process_response_chunk(http_context_id, Bytes::new(), true).await
                };
                match terminal {
                    Ok(terminal) if !terminal.bytes.is_empty() => {
                        if sender.send(Ok(Frame::data(terminal.bytes))).await.is_err() {
                            finish_stream_context(&vm, http_context_id).await;
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = sender.send(Err(Box::new(error))).await;
                    }
                }
                finish_stream_context(&vm, http_context_id).await;
                return;
            }
        }
    }
}

/// 尝试执行 trailers/lifecycle；失败时仍强制移除 context，避免占用 VM 容量。
async fn finish_stream_context(vm: &Arc<Mutex<Vm>>, http_context_id: u32) {
    let mut vm = vm.lock().await;
    if let Err(error) = vm.finish_response_stream(http_context_id).await {
        tracing::warn!(target: "spacegate_plugin_wasm", http_context_id, error = %error, "failed to finish streamed Proxy-Wasm context");
        vm.abort_response_stream(http_context_id);
    }
}
