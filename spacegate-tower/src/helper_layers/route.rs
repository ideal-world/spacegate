use std::{collections::VecDeque, convert::Infallible, ops::IndexMut};

use futures_util::future::BoxFuture;
pub use hyper::http::request::Parts;
use hyper::{Request, Response};

use tower_service::Service;

use crate::{extension::Matched, SgBody};

pub trait Router: Clone {
    type Index: Clone;
    fn route(&self, req: &Request<SgBody>) -> Option<Self::Index>;
    fn all_indexes(&self) -> VecDeque<Self::Index>;
}

#[derive(Debug, Clone)]
pub struct Route<S, R, F>
where
    R: Router,
{
    services: S,
    fallback: F,
    router: R,
    unready_services: VecDeque<R::Index>,
}

impl<S, R, F> Route<S, R, F>
where
    R: Router,
    S: IndexMut<R::Index>,
{
    pub fn new(services: S, router: R, fallback: F) -> Self {
        let unready_services = R::all_indexes(&router);
        Self {
            services,
            router,
            fallback,
            unready_services,
        }
    }
}

impl<S, R, F> Service<Request<SgBody>> for Route<S, R, F>
where
    R: Router + Send + Sync + 'static,
    R::Index: Send + Sync + 'static,
    S: IndexMut<R::Index>,
    S::Output: Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    F: Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <F as Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
    <S::Output as Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
{
    type Error = Infallible;
    type Response = Response<SgBody>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        while let Some(idx) = self.unready_services.pop_front() {
            let service = &mut self.services[idx.clone()];
            if let std::task::Poll::Ready(result) = service.poll_ready(cx) {
                result?;
                continue;
            } else {
                self.unready_services.push_back(idx);
                return std::task::Poll::Pending;
            }
        }
        self.fallback.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<SgBody>) -> Self::Future {
        let fut: Self::Future = if let Some(index) = self.router.route(&req) {
            req.extensions_mut().insert(Matched {
                router: self.router.clone(),
                index: index.clone(),
            });
            let fut = self.services.index_mut(index.clone()).call(req);
            self.unready_services.push_back(index);

            Box::pin(fut)
        } else {
            let fut = self.fallback.call(req);
            Box::pin(fut)
        };
        fut
    }
}
