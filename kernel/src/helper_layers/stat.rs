use futures_util::Future;
use hyper::{Request, Response};
use std::{convert::Infallible, sync::Arc, task::ready};

use crate::SgBody;

#[derive(Debug, Clone)]
pub struct StatLayer<P> {
    policy: Arc<P>,
}

impl<P> StatLayer<P> {
    pub fn new(policy: impl Into<Arc<P>>) -> Self {
        Self { policy: policy.into() }
    }
}

pub trait Policy {
    fn on_request(&self, req: &Request<SgBody>);
    fn on_response(&self, resp: &Response<SgBody>);
}

#[derive(Debug, Clone)]
pub struct Stat<P, S> {
    policy: Arc<P>,
    inner: S,
}

impl<P, S> Stat<P, S> {
    pub fn new(policy: impl Into<Arc<P>>, inner: S) -> Self {
        Self { policy: policy.into(), inner }
    }
}

impl<P, S> tower_layer::Layer<S> for StatLayer<P>
where
    P: Policy + Clone,
{
    type Service = Stat<P, S>;

    fn layer(&self, inner: S) -> Self::Service {
        Stat::new(self.policy.clone(), inner)
    }
}

impl<P, S> hyper::service::Service<Request<SgBody>> for Stat<P, S>
where
    P: Policy,
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible>,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = ResponseFuture<S::Future, P>;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        self.policy.on_request(&req);
        let fut = self.inner.call(req);
        ResponseFuture::new(fut, self.policy.clone())
    }
}

pin_project_lite::pin_project! {
    pub struct ResponseFuture<F, P> {
        #[pin]
        inner: F,
        policy: Arc<P>,
    }

}

impl<F, P> ResponseFuture<F, P> {
    pub fn new(inner: F, policy: Arc<P>) -> Self {
        Self { inner, policy }
    }
}

impl<F, P> Future for ResponseFuture<F, P>
where
    F: Future<Output = Result<Response<SgBody>, Infallible>>,
    P: Policy,
{
    type Output = Result<Response<SgBody>, Infallible>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let this = self.project();
        let response = ready!(this.inner.poll(cx)).expect("infallible");
        this.policy.on_response(&response);
        std::task::Poll::Ready(Ok(response))
    }
}
