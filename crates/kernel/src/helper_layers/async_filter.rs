pub mod dump;
use std::{convert::Infallible, task::ready};

use futures_util::Future;
use hyper::{Request, Response};
use pin_project_lite::pin_project;
use tower_layer::Layer;

use crate::SgBody;

pub trait AsyncFilter: Clone {
    type Future: Future<Output = Result<Request<SgBody>, Response<SgBody>>> + Send + 'static;
    fn filter(&self, req: Request<SgBody>) -> Self::Future;
}

#[derive(Debug, Clone)]
pub struct AsyncFilterRequestLayer<F> {
    filter: F,
}

impl<F> AsyncFilterRequestLayer<F> {
    pub fn new(filter: F) -> Self {
        Self { filter }
    }
}

impl<F, S> Layer<S> for AsyncFilterRequestLayer<F>
where
    F: AsyncFilter,
{
    type Service = AsyncFilterRequest<F, S>;

    fn layer(&self, inner: S) -> Self::Service {
        AsyncFilterRequest {
            filter: self.filter.clone(),
            inner,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsyncFilterRequest<F, S> {
    filter: F,
    inner: S,
}

pin_project! {
    #[project = FilterResponseFutureStateProj]
    enum FilterResponseFutureState<S, F> {
        Filter {
            #[pin]
            fut: F
        },
        Inner {
            #[pin]
            fut: S
        },
    }
}

pin_project! {
    pub struct FilterResponseFuture<S, F>
    where S: hyper::service::Service<Request<SgBody>>, F: AsyncFilter
    {
        #[pin]
        state: FilterResponseFutureState<S::Future, F::Future>,
        inner_service: S
    }
}

impl<S, F> Future for FilterResponseFuture<S, F>
where
    F: AsyncFilter,
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible>,
{
    type Output = Result<Response<SgBody>, Infallible>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let mut this = self.project();
        loop {
            match this.state.as_mut().project() {
                FilterResponseFutureStateProj::Filter { fut } => match ready!(fut.poll(cx)) {
                    Ok(req) => this.state.set(FilterResponseFutureState::Inner {
                        fut: this.inner_service.call(req),
                    }),
                    Err(resp) => return std::task::Poll::Ready(Ok(resp)),
                },
                FilterResponseFutureStateProj::Inner { fut } => {
                    let resp = ready!(fut.poll(cx)).expect("infallible");
                    return std::task::Poll::Ready(Ok(resp));
                }
            }
        }
    }
}

impl<F, S> hyper::service::Service<Request<SgBody>> for AsyncFilterRequest<F, S>
where
    F: AsyncFilter,
    S: Clone + hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>>,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = FilterResponseFuture<S, F>;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        let inner = self.inner.clone();
        let filter = self.filter.clone();
        // filter the request

        FilterResponseFuture {
            state: FilterResponseFutureState::Filter { fut: filter.filter(req) },
            inner_service: inner,
        }
    }
}
