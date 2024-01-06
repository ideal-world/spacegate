pub mod response_anyway;

use std::{convert::Infallible, future::Ready};

use crate::SgBody;
use hyper::{Request, Response};
use tower_layer::Layer;
use tower_service::Service;

pub trait Filter: Clone {
    fn filter(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>>;
}

#[derive(Debug, Clone)]

pub struct FilterRequestLayer<F> {
    filter: F,
}

impl<F> FilterRequestLayer<F> {
    pub fn new(filter: F) -> Self {
        Self { filter }
    }
}

impl<F, S> Layer<S> for FilterRequestLayer<F>
where
    F: Filter,
{
    type Service = FilterRequest<F, S>;

    fn layer(&self, inner: S) -> Self::Service {
        FilterRequest {
            filter: self.filter.clone(),
            inner,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilterRequest<F, S> {
    filter: F,
    inner: S,
}

impl<F, S> Service<Request<SgBody>> for FilterRequest<F, S>
where
    F: Filter,
    S: Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>>,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = futures_util::future::Either<Ready<Result<Self::Response, Self::Error>>, S::Future>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        match self.filter.filter(req) {
            Ok(req) => futures_util::future::Either::Right(self.inner.call(req)),
            Err(resp) => futures_util::future::Either::Left(std::future::ready(Ok(resp))),
        }
    }
}