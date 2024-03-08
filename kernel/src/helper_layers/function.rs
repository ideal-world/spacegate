use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use futures_util::{future::Map, Future};
use hyper::{service::Service, Request, Response};
use std::convert::Infallible;
use std::sync::Arc;
use tower_layer::Layer;

use crate::{BoxHyperService, SgBody};

pub trait FnLayerMethod {
    fn call(&self, req: Request<SgBody>, next: Next) -> impl Future<Output = Response<SgBody>> + Send;
}

pub struct FnLayer<F, Fut>
where
    F: Fn(Request<SgBody>, Next) -> Fut,
    Fut: Future<Output = Response<SgBody>>,
{
    f: F,
}

impl<F, Fut> FnLayer<F, Fut>
where
    F: Fn(Request<SgBody>, Next) -> Fut,
    Fut: Future<Output = Response<SgBody>>,
{
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl FnLayer<Box<dyn Fn(Request<SgBody>, Next) -> BoxFuture<'static, Response<SgBody>>>, BoxFuture<'static, Response<SgBody>>> {
    pub fn from_method<M>(m: M) -> Self
    where
        M: FnLayerMethod + 'static + Send + Sync,
    {
        let method = Arc::new(m);
        Self::new(Box::new(move |req, next| {
            let method = method.clone();
            Box::pin(async move { method.call(req, next).await })
        }))
    }
}

impl<F, Fut, S> Layer<S> for FnLayer<F, Fut>
where
    F: Fn(Request<SgBody>, Next) -> Fut + Clone,
    Fut: Future<Output = Response<SgBody>>,
    S: Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + Clone + 'static,
    <S as Service<Request<SgBody>>>::Future: Future<Output = Result<Response<SgBody>, Infallible>> + 'static + Send,
{
    type Service = FnService<F, Fut>;

    fn layer(&self, inner: S) -> Self::Service {
        FnService {
            f: self.f.clone(),
            inner: BoxHyperService::new(inner),
        }
    }
}

pub struct FnService<F, Fut>
where
    F: Fn(Request<SgBody>, Next) -> Fut,
    Fut: Future<Output = Response<SgBody>>,
{
    f: F,
    inner: BoxHyperService,
}

impl<F, Fut> Service<Request<SgBody>> for FnService<F, Fut>
where
    F: Fn(Request<SgBody>, Next) -> Fut,
    Fut: Future<Output = Response<SgBody>>,
{
    type Response = Response<SgBody>;

    type Error = Infallible;

    type Future = Map<Fut, fn(Response<SgBody>) -> Result<Response<SgBody>, Infallible>>;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        let next = Next { inner: self.inner.clone() };
        let fut = (self.f)(req, next);
        fut.map(Result::Ok)
    }
}

pub struct Next {
    inner: BoxHyperService,
}

impl Next {
    pub async fn call(self, req: Request<SgBody>) -> Response<SgBody> {
        // just infallible
        unsafe { self.inner.call(req).await.unwrap_unchecked() }
    }
}