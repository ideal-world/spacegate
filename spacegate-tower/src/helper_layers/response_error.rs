use std::{convert::Infallible, future::Future, marker, sync::Arc, task::ready};

use hyper::{Request, Response};
use pin_project_lite::pin_project;
use tower_layer::Layer;
use tower_service::Service;

use crate::SgBody;
#[derive(Debug, Clone)]
pub struct ResponseErrorLayer<F> {
    formatter: Arc<F>,
}

impl<F> ResponseErrorLayer<F> {
    pub fn new(formatter: F) -> Self {
        Self { formatter: Arc::new(formatter) }
    }
}

impl Default for ResponseErrorLayer<DefaultErrorFormatter> {
    fn default() -> Self {
        Self {
            formatter: Arc::new(DefaultErrorFormatter),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DefaultErrorFormatter;

impl ErrorFormatter for DefaultErrorFormatter {
    fn format(&self, err: impl std::error::Error) -> String {
        err.to_string()
    }
}

pub trait ErrorFormatter {
    fn format(&self, err: impl std::error::Error) -> String;
}

impl<S, F> Layer<S> for ResponseErrorLayer<F>
where
    F: ErrorFormatter,
{
    type Service = ResponseError<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        ResponseError {
            inner,
            formatter: self.formatter.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResponseError<S, F = DefaultErrorFormatter> {
    formatter: Arc<F>,
    inner: S,
}

pin_project! {
    pub struct ResponseErrorFuture<E, F, FMT> {
        formatter: Arc<FMT>,
        error: marker::PhantomData<E>,
        #[pin]
        inner: F,
    }
}

impl<E, F, FMT> ResponseErrorFuture<E, F, FMT> {
    pub fn new(formatter: impl Into<Arc<FMT>>, fut: F) -> Self {
        ResponseErrorFuture {
            formatter: formatter.into(),
            error: marker::PhantomData,
            inner: fut,
        }
    }
}

impl<E, F, FMT> Future for ResponseErrorFuture<E, F, FMT>
where
    F: Future<Output = Result<Response<SgBody>, E>>,
    E: std::error::Error,
    FMT: ErrorFormatter,
{
    type Output = Result<Response<SgBody>, Infallible>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        use crate::SgResponseExt;
        let this = self.project();
        let inner_call_result = ready!(this.inner.poll(cx));
        match inner_call_result {
            Ok(resp) => std::task::Poll::Ready(Ok(resp)),
            Err(e) => std::task::Poll::Ready(Ok(Response::from_error(e, this.formatter.as_ref()))),
        }
    }
}

impl<S, FMT> hyper::service::Service<Request<SgBody>> for ResponseError<S, FMT> 
where
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>> + Send + Sync + 'static,
    S::Error: std::error::Error,
    FMT: ErrorFormatter,
{
    type Response = Response<SgBody>;

    type Error = Infallible;

    type Future = ResponseErrorFuture<S::Error, S::Future, FMT>;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        let fut = self.inner.call(req);
        ResponseErrorFuture {
            error: marker::PhantomData,
            inner: fut,
            formatter: self.formatter.clone(),
        }
    }
}


impl<S, FMT> Service<Request<SgBody>> for ResponseError<S, FMT>
where
    S: Service<Request<SgBody>, Response = Response<SgBody>> + Send + Sync + 'static,
    S::Error: std::error::Error,
    FMT: ErrorFormatter,
{
    type Response = Response<SgBody>;

    type Error = Infallible;

    type Future = ResponseErrorFuture<S::Error, S::Future, FMT>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map(|_| Ok(()))
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        let fut = self.inner.call(req);
        ResponseErrorFuture {
            error: marker::PhantomData,
            inner: fut,
            formatter: self.formatter.clone(),
        }
    }
}
