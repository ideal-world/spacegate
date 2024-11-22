use hyper::service::Service;
use tower_layer::Layer;

use crate::{injector::Inject, SgRequest};
#[derive(Debug, Clone)]
pub struct InjectorLayer<I> {
    inject: I,
}

#[derive(Debug, Clone)]
pub struct Injector<I, S> {
    inject: I,
    inner: S,
}

impl<I> InjectorLayer<I> {
    pub fn new(inject: I) -> Self {
        Self { inject }
    }
}

impl<I, S> Injector<I, S> {
    pub fn new(inject: I, inner: S) -> Self {
        Self { inject, inner }
    }
}

impl<S, I> Layer<S> for InjectorLayer<I>
where
    I: Clone,
{
    type Service = Injector<I, S>;

    fn layer(&self, inner: S) -> Self::Service {
        Injector::new(self.inject.clone(), inner)
    }
}

impl<I, S> Service<SgRequest> for Injector<I, S>
where
    I: Inject,
    S: Service<SgRequest>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&self, mut req: SgRequest) -> Self::Future {
        if let Err(e) = self.inject.inject(&mut req) {
            tracing::error!("inject error: {}", e);
        }
        self.inner.call(req)
    }
}
