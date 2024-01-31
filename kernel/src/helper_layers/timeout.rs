use std::{
    convert::Infallible,
    time::{Duration, Instant},
};

use futures_util::Future;
use hyper::{Request, Response};
use tower_layer::Layer;

use crate::SgBody;
#[derive(Clone)]
pub struct TimeoutLayer {
    /// timeout duration, none value means no timeout
    pub timeout: Option<Duration>,
    pub timeout_response: hyper::body::Bytes,
}

impl<S> Layer<S> for TimeoutLayer {
    type Service = Timeout<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Timeout {
            inner,
            timeout: self.timeout,
            timeout_response: self.timeout_response.clone(),
        }
    }
}

#[derive(Clone)]
pub struct Timeout<S> {
    inner: S,
    timeout: Option<Duration>,
    timeout_response: hyper::body::Bytes,
}

impl TimeoutLayer {
    pub fn new(timeout: Option<Duration>) -> Self {
        Self {
            timeout,
            timeout_response: hyper::body::Bytes::default(),
        }
    }
    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout;
    }
}

impl<S> Timeout<S> {
    pub fn new(timeout: Option<Duration>, timeout_response: hyper::body::Bytes, inner: S) -> Self {
        Self { inner, timeout, timeout_response }
    }
}

impl<S> hyper::service::Service<Request<SgBody>> for Timeout<S>
where
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <S as hyper::service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Response = Response<SgBody>;

    type Error = Infallible;

    type Future = TimeoutFuture<S::Future>;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        TimeoutFuture {
            inner: self.inner.call(req),
            timeout_at: self.timeout.map(|d| Instant::now() + d),
            timeout_response: self.timeout_response.clone(),
        }
    }
}

pin_project_lite::pin_project! {
    pub struct TimeoutFuture<F> {
        #[pin]
        inner: F,
        timeout_at: Option<Instant>,
        timeout_response: hyper::body::Bytes,
    }
}

impl<F> Future for TimeoutFuture<F>
where
    F: Future<Output = Result<Response<SgBody>, Infallible>> + Send + 'static,
{
    type Output = Result<Response<SgBody>, Infallible>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let this = self.project();
        if let Some(timeout_at) = this.timeout_at {
            if Instant::now() >= *timeout_at {
                let response = Response::builder().status(hyper::StatusCode::GATEWAY_TIMEOUT).body(SgBody::full(this.timeout_response.clone())).expect("invalid response");
                return std::task::Poll::Ready(Ok(response));
            }
        }
        this.inner.poll(cx)
    }
}
