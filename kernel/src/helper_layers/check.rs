#[cfg(feature = "ext-redis")]
pub mod redis;

use std::{convert::Infallible, future::ready, marker::PhantomData};

use futures_util::{future::BoxFuture, Future};
use hyper::{Request, Response, StatusCode};
use tower_layer::Layer;

use crate::{Marker, SgBody, SgResponseExt};

pub trait Check<M>: Sync + Send + 'static
where
    M: Marker + Send + Sync + 'static,
{
    fn check(&self, _marker: &M) -> impl Future<Output = bool> + Send {
        ready(true)
    }
    fn on_forbidden(&self, _marker: M) -> Response<SgBody> {
        Response::with_code_message(StatusCode::FORBIDDEN, "forbidden")
    }
    fn on_missing(&self) -> Response<SgBody> {
        Response::with_code_message(StatusCode::UNAUTHORIZED, "unauthorized")
    }
    fn on_pass(&self, request: Request<SgBody>) -> Request<SgBody> {
        request
    }
    fn on_response(&self, _marker: M, resp: Response<SgBody>) -> Response<SgBody> {
        resp
    }
}

pub struct CheckLayer<C, M> {
    check: C,
    marker: PhantomData<fn() -> M>,
}

impl<C, M> CheckLayer<C, M> {
    pub fn new(check: C) -> Self {
        Self { check, marker: PhantomData }
    }
}

impl<C, S, M> Layer<S> for CheckLayer<C, M>
where
    C: Clone,
{
    type Service = CheckService<C, S, M>;

    fn layer(&self, inner: S) -> Self::Service {
        CheckService {
            check: self.check.clone(),
            service: inner,
            marker: PhantomData,
        }
    }
}

impl<C, M> Clone for CheckLayer<C, M>
where
    C: Clone,
{
    fn clone(&self) -> Self {
        Self {
            check: self.check.clone(),
            marker: PhantomData,
        }
    }
}

pub struct CheckService<C, S, M> {
    check: C,
    service: S,
    marker: std::marker::PhantomData<fn() -> M>,
}

impl<C, S, M> CheckService<C, S, M> {
    pub fn new(check: C, service: S) -> Self {
        Self {
            check,
            service,
            marker: Default::default(),
        }
    }
}

impl<C, S, M> Clone for CheckService<C, S, M>
where
    C: Clone,
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            check: self.check.clone(),
            service: self.service.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

impl<C, S, M> hyper::service::Service<Request<SgBody>> for CheckService<C, S, M>
where
    M: Marker,
    C: Check<M> + Clone,
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Clone + Send + Sync + 'static,
    <S as hyper::service::Service<Request<SgBody>>>::Future: Send,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Response<SgBody>, Infallible>>;

    fn call(&self, request: Request<SgBody>) -> Self::Future {
        let cloned = self.clone();
        let marker = M::extract(&request);
        Box::pin(async move {
            let checker = &cloned.check;
            if let Some(marker) = marker {
                if checker.check(&marker).await {
                    let resp = cloned.service.call(checker.on_pass(request)).await.expect("infallible");
                    Ok(checker.on_response(marker, resp))
                } else {
                    Ok(checker.on_forbidden(marker))
                }
            } else {
                Ok(checker.on_missing())
            }
        })
    }
}
