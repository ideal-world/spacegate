use std::{convert::Infallible, ops::Index};

use futures_util::future::BoxFuture;
pub use hyper::http::request::Parts;
use hyper::{Request, Response};
use tracing::instrument;

use crate::{extension::Matched, SgBody};

pub trait Router: Clone {
    type Index: Clone;
    fn route(&self, req: &mut Request<SgBody>) -> Option<Self::Index>;
}

#[derive(Debug, Clone)]
pub struct RouterService<S, R, F>
where
    R: Router,
{
    services: S,
    fallback: F,
    router: R,
}

impl<S, R, F> RouterService<S, R, F>
where
    R: Router,
    S: Index<R::Index>,
{
    pub fn new(services: S, router: R, fallback: F) -> Self {
        Self { services, router, fallback }
    }
}

impl<S, R, F> hyper::service::Service<Request<SgBody>> for RouterService<S, R, F>
where
    R: Router + Send + Sync + 'static,
    R::Index: Send + Sync + 'static + Clone,
    S: Index<R::Index>,
    S::Output: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    F: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <F as hyper::service::Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
    <S::Output as hyper::service::Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
{
    type Error = Infallible;
    type Response = Response<SgBody>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    #[instrument(skip_all, fields(http.uri =? req.uri(), http.method =? req.method()))]
    fn call(&self, mut req: Request<SgBody>) -> Self::Future {
        tracing::trace!("entered");
        let fut: Self::Future = if let Some(index) = self.router.route(&mut req) {
            req.extensions_mut().insert(Matched {
                index: index.clone(),
                router: self.router.clone(),
            });
            let fut = self.services.index(index).call(req);
            Box::pin(async move {
                let result = fut.await;
                tracing::trace!("finished");
                result
            })
        } else {
            let fut = self.fallback.call(req);
            Box::pin(async move {
                let result = fut.await;
                tracing::trace!("finished");
                result
            })
        };
        fut
    }
}
