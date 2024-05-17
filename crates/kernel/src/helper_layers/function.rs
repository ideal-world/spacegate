use futures_util::future::BoxFuture;
use futures_util::Future;
use hyper::{service::Service, Request, Response};
use std::{convert::Infallible, sync::Arc};
use tower_layer::Layer;

use crate::{ArcHyperService, SgBody};

/// see [`FnLayer`]
pub trait FnLayerMethod: Send + 'static {
    fn call(&self, req: Request<SgBody>, inner: Inner) -> impl Future<Output = Response<SgBody>> + Send;
}

impl<T> FnLayerMethod for Arc<T>
where
    T: FnLayerMethod + std::marker::Sync,
{
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Response<SgBody> {
        self.as_ref().call(req, inner).await
    }
}

/// see [`FnLayer`]
#[derive(Debug)]
pub struct Closure<F, Fut>
where
    F: Fn(Request<SgBody>, Inner) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Response<SgBody>> + Send + 'static,
{
    pub f: F,
}

impl<F, Fut> Closure<F, Fut>
where
    F: Fn(Request<SgBody>, Inner) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Response<SgBody>> + Send + 'static,
{
    pub const fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F, Fut> From<F> for Closure<F, Fut>
where
    F: Fn(Request<SgBody>, Inner) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Response<SgBody>> + Send + 'static,
{
    fn from(value: F) -> Self {
        Closure { f: value }
    }
}

impl<F, Fut> Clone for Closure<F, Fut>
where
    F: Fn(Request<SgBody>, Inner) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Response<SgBody>> + Send + 'static,
{
    fn clone(&self) -> Self {
        Self { f: self.f.clone() }
    }
}

impl<F, Fut> FnLayerMethod for Closure<F, Fut>
where
    F: Fn(Request<SgBody>, Inner) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Response<SgBody>> + Send + 'static,
{
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Response<SgBody> {
        (self.f)(req, inner).await
    }
}

/// A functional layer
///
/// This is an example of how to create a layer that adds a header to the response:
/// ```
/// # use spacegate_kernel::helper_layers::function::FnLayer;
/// # use hyper::http::header::HeaderValue;
/// let layer = FnLayer::new_closure(move |req, inner| {
///    async move {
///        let mut resp = inner.call(req).await;
///        resp.headers_mut().insert("server", HeaderValue::from_static("potato"));
///        resp
///    }
/// });
/// ```
///
/// Or you can use a struct that implements `FnLayerMethod`:
/// ```
/// # use spacegate_kernel::{helper_layers::function::{FnLayer, FnLayerMethod, Inner}, SgRequest, SgResponse};
/// # use hyper::http::header::HeaderValue;
/// struct MyPlugin;
/// impl FnLayerMethod for MyPlugin {
///    async fn call(&self, req: SgRequest, inner: Inner) -> SgResponse {
///       let mut resp = inner.call(req).await;
///       resp.headers_mut().insert("server", HeaderValue::from_static("potato"));
///       resp
///    }
/// }
/// let layer = FnLayer::new(MyPlugin);
/// ```
#[derive(Debug, Clone)]
pub struct FnLayer<M> {
    method: M,
}

impl<M> FnLayer<M> {
    pub const fn new(method: M) -> Self {
        Self { method }
    }
}

impl<F, Fut> FnLayer<Closure<F, Fut>>
where
    F: Fn(Request<SgBody>, Inner) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Response<SgBody>> + Send + 'static,
{
    pub const fn new_closure(f: F) -> Self {
        Self::new(Closure::new(f))
    }
}

impl<M, S> Layer<S> for FnLayer<M>
where
    M: FnLayerMethod + Clone,
    S: Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + Clone + 'static,
    <S as Service<Request<SgBody>>>::Future: Future<Output = Result<Response<SgBody>, Infallible>> + 'static + Send,
{
    type Service = FnService<M>;

    fn layer(&self, inner: S) -> Self::Service {
        FnService {
            m: self.method.clone(),
            inner: ArcHyperService::new(inner),
        }
    }
}

/// The corresponded server for [`FnLayer`]
#[derive(Debug, Clone)]
pub struct FnService<M> {
    m: M,
    inner: ArcHyperService,
}

impl<M> Service<Request<SgBody>> for FnService<M>
where
    M: FnLayerMethod + Clone,
{
    type Response = Response<SgBody>;

    type Error = Infallible;

    type Future = BoxFuture<'static, Result<Response<SgBody>, Infallible>>;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        let next = Inner { inner: self.inner.clone() };
        let method = self.m.clone();
        Box::pin(async move { Ok(method.call(req, next).await) })
    }
}

/// A shared hyper service wrapper
#[derive(Clone)]
pub struct Inner {
    inner: ArcHyperService,
}

impl Inner {
    pub fn new(inner: ArcHyperService) -> Self {
        Inner { inner }
    }
    /// Call the inner service and get the response
    pub async fn call(self, req: Request<SgBody>) -> Response<SgBody> {
        // just infallible
        unsafe { self.inner.call(req).await.unwrap_unchecked() }
    }
    /// Unwrap the inner service
    pub fn into_inner(self) -> ArcHyperService {
        self.inner
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, sync::Arc};

    use hyper::{header::HeaderValue, StatusCode};
    #[derive(Debug, Default, Clone)]
    pub struct MyPlugin {
        status_message: HashMap<StatusCode, String>,
    }

    impl FnLayerMethod for MyPlugin {
        async fn call(&self, req: Request<SgBody>, inner: Inner) -> Response<SgBody> {
            let host = req.headers().get("host");
            if let Some(Ok(host)) = host.map(HeaderValue::to_str) {
                println!("{host}");
            }
            let resp = inner.call(req).await;
            if let Some(message) = self.status_message.get(&resp.status()) {
                println!("{message}");
            }
            resp
        }
    }
    use crate::BoxLayer;

    use super::*;
    #[test]
    fn test_fn_layer() {
        let status_message = Arc::new(<HashMap<StatusCode, String>>::default());
        let boxed_layer = BoxLayer::new(FnLayer::new(MyPlugin::default()));
        let boxed_layer2 = BoxLayer::new(FnLayer::new_closure(move |req, inner| {
            let host = req.headers().get("host");
            if let Some(Ok(host)) = host.map(HeaderValue::to_str) {
                println!("{host}");
            }
            let status_message = status_message.clone();
            async move {
                let resp = inner.call(req).await;
                if let Some(message) = status_message.get(&resp.status()) {
                    println!("{message}");
                }
                resp
            }
        }));
        drop(boxed_layer);
        drop(boxed_layer2);
    }
}
