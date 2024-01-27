use std::convert::Infallible;

use hyper::{Request, Response};
use tower_layer::Layer;

use crate::SgBody;

pub struct MapRequestLayer<F> {
    map: F,
}

impl<F> MapRequestLayer<F> {
    pub fn new(map: F) -> Self {
        Self { map }
    }
}

impl<F, S> Layer<S> for MapRequestLayer<F>
where
    F: Fn(Request<SgBody>) -> Request<SgBody> + Clone,
{
    type Service = MapRequest<F, S>;

    fn layer(&self, inner: S) -> Self::Service {
        MapRequest { map: self.map.clone(), inner }
    }
}

pub struct MapRequest<F, S> {
    map: F,
    inner: S,
}

impl<F, S> Clone for MapRequest<F, S>
where
    F: Clone,
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            map: self.map.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<F, S> hyper::service::Service<Request<SgBody>> for MapRequest<F, S>
where
    F: Fn(Request<SgBody>) -> Request<SgBody> + Clone,
    S: hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>>,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = S::Future;

    fn call(&self, request: Request<SgBody>) -> Self::Future {
        self.inner.call((self.map)(request))
    }
}
