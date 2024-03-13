use std::{convert::Infallible, future::ready};

use futures_util::{future::BoxFuture, Future};
use hyper::{Request, Response, StatusCode};

use crate::{Marker, ReqOrResp, SgBody, SgResponseExt};

pub trait Check<M>: Sync + Send + 'static
where
    M: Marker + Send + Sync + 'static,
{
    fn check(&self, _marker: M) -> impl Future<Output = bool> + Send {
        ready(true)
    }
    fn on_pass(&self) -> Response<SgBody> {
        Response::with_code_message(StatusCode::FORBIDDEN, "forbidden")
    }
    fn on_missing(&self) -> Response<SgBody> {
        Response::with_code_message(StatusCode::UNAUTHORIZED, "unauthorized")
    }
    fn on_response(&self, resp: Response<SgBody>) -> Response<SgBody> {
        resp
    }
    fn on_request(&self, req: Request<SgBody>) -> impl Future<Output = ReqOrResp> + Send {
        Box::pin(async move {
            if let Some(authority) = M::extract(&req) {
                if self.check(authority).await {
                    Ok(req)
                } else {
                    Err(self.on_pass())
                }
            } else {
                Err(self.on_missing())
            }
        })
    }
}

pub struct CheckLayer<C> {
    check: C,
}

impl<C> CheckLayer<C> {
    pub fn new(check: C) -> Self {
        Self { check }
    }
}

impl<C> Clone for CheckLayer<C>
where
    C: Clone,
{
    fn clone(&self) -> Self {
        Self { check: self.check.clone() }
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
        Box::pin(async move {
            match cloned.check.on_request(request).await {
                Ok(req) => {
                    let resp = cloned.service.call(req).await.expect("infallible");
                    Ok(cloned.check.on_response(resp))
                }
                Err(resp) => Ok(resp),
            }
        })
    }
}
