use std::convert::Infallible;

use futures_util::TryFutureExt;
use hyper::{Request, Response};
use tower_layer::Layer;

use crate::SgBody;

pub struct MapResponseLayer<F> {
    map: F,
}

impl<F> MapResponseLayer<F> {
    pub fn new(map: F) -> Self {
        Self { map }
    }
}

impl<F, S> Layer<S> for MapResponseLayer<F>
where
    F: Fn(Response<SgBody>) -> Response<SgBody> + Clone,
{
    type Service = MapResponse<F, S>;

    fn layer(&self, inner: S) -> Self::Service {
        MapResponse { map: self.map.clone(), inner }
    }
}

pub struct MapResponse<F, S> {
    map: F,
    inner: S,
}

impl<F, S> Clone for MapResponse<F, S>
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

impl<F, S> hyper::service::Service<Request<SgBody>> for MapResponse<F, S>
where
    F: Fn(Response<SgBody>) -> Response<SgBody> + Clone,
    S: hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>>,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = futures_util::future::MapOk<S::Future, F>;

    fn call(&self, request: Request<SgBody>) -> Self::Future {
        self.inner.call(request).map_ok(self.map.clone())
    }
}
