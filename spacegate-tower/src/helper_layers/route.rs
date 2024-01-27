use std::{collections::VecDeque, convert::Infallible, ops::{Index}};

use futures_util::future::BoxFuture;
pub use hyper::http::request::Parts;
use hyper::{Request, Response};

use tower_service::Service;

use crate::{extension::Matched, SgBody};

pub trait Router: Clone {
    type Index: Clone;
    fn route(&self, req: &Request<SgBody>) -> Option<Self::Index>;
}

#[derive(Debug, Clone)]
pub struct Route<S, R, F>
where
    R: Router,
{
    services: S,
    fallback: F,
    router: R,
}

impl<S, R, F> Route<S, R, F>
where
    R: Router,
    S: Index<R::Index>,
{
    pub fn new(services: S, router: R, fallback: F) -> Self {
        Self {
            services,
            router,
            fallback,
        }
    }
}

impl<S, R, F> hyper::service::Service<Request<SgBody>> for Route<S, R, F>
where
    R: Router + Send + Sync + 'static,
    R::Index: Send + Sync + 'static,
    S: Index<R::Index>,
    S::Output: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    F: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <F as hyper::service::Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
    <S::Output as hyper::service::Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
{
    type Error = Infallible;
    type Response = Response<SgBody>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    fn call(&self, req: Request<SgBody>) -> Self::Future {
        let fut: Self::Future = if let Some(index) = self.router.route(&req) {
            let fut = self.services.index(index).call(req);
            Box::pin(fut)
        } else {
            let fut = self.fallback.call(req);
            Box::pin(fut)
        };
        fut
    }
}