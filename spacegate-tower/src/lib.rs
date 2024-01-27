#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo)]

// pub mod config;
pub mod body;
pub mod extension;
pub mod header;
pub mod helper_layers;
pub mod layers;
pub mod listener;
pub mod service;
pub mod utils;

pub use body::SgBody;
use extension::Reflect;
use std::{convert::Infallible, fmt, sync::Arc};
pub use tower_layer::Layer;
pub use tower_service::Service;
pub use service::BoxHyperService;
use helper_layers::response_error::ErrorFormatter;

use hyper::{body::Bytes, Request, Response, StatusCode};

use tower::util::BoxCloneService;
use tower_layer::layer_fn;
use utils::fold_sg_layers::fold_sg_layers;

pub trait SgRequestExt {
    fn with_reflect(&mut self);
    // fn into_context(self) -> (SgContext, Request<BoxBody<Bytes, hyper::Error>>);
}

impl SgRequestExt for Request<SgBody> {
    fn with_reflect(&mut self) {
        self.extensions_mut().insert(Reflect::new());
    }
    // fn into_context(self) -> (SgContext, Request<BoxBody<Bytes, hyper::Error>>) {
    //     let (parts, body) = self.into_parts();
    //     let (context, body) = body.into_context();
    //     let real_body = Request::from_parts(parts, body);
    //     (context, real_body)
    // }
}

pub trait SgResponseExt {
    fn with_code_message(code: StatusCode, message: impl Into<Bytes>) -> Self;
    fn internal_error<E: std::error::Error>(e: E) -> Self
    where
        Self: Sized,
    {
        let message = e.to_string();
        tracing::debug!(message, "[Sg] internal error");
        Self::with_code_message(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
    fn from_error<E: std::error::Error, F: ErrorFormatter>(e: E, formatter: &F) -> Self
    where
        Self: Sized,
    {
        let message = formatter.format(&e);
        tracing::debug!(message, "[Sg] internal error");
        Self::with_code_message(StatusCode::INTERNAL_SERVER_ERROR, formatter.format(&e))
    }
}

impl SgResponseExt for Response<SgBody> {
    fn with_code_message(code: StatusCode, message: impl Into<Bytes>) -> Self {
        let body = SgBody::full(message);
        let mut resp = Response::builder().status(code).body(body).expect("response builder error");
        resp.extensions_mut().insert(Reflect::new());
        resp
    }
}

pub type ReqOrResp = Result<Request<SgBody>, Response<SgBody>>;

pub struct SgBoxLayer {
    boxed: Arc<dyn Layer<BoxHyperService, Service = BoxHyperService> + Send + Sync + 'static>,
}

impl FromIterator<SgBoxLayer> for SgBoxLayer {
    fn from_iter<T: IntoIterator<Item = SgBoxLayer>>(iter: T) -> Self {
        fold_sg_layers(iter.into_iter())
    }
}

impl<'a> FromIterator<&'a SgBoxLayer> for SgBoxLayer {
    fn from_iter<T: IntoIterator<Item = &'a SgBoxLayer>>(iter: T) -> Self {
        fold_sg_layers(iter.into_iter().cloned())
    }
}

impl SgBoxLayer {
    /// Create a new [`BoxLayer`].
    pub fn new<L>(inner_layer: L) -> Self
    where
        L: Layer<BoxHyperService> + Send + Sync + 'static,
        L::Service: Clone + hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + Sync + 'static,
        <L::Service as hyper::service::Service<Request<SgBody>>>::Future: Send + 'static,
    {
        let layer = layer_fn(move |inner: BoxHyperService| {
            let out = inner_layer.layer(inner);
            BoxHyperService::new(out)
        });

        Self { boxed: Arc::new(layer) }
    }
    pub fn layer_boxed(&self, inner: BoxHyperService) -> BoxHyperService {
        self.boxed.layer(inner)
    }
}

impl<S> Layer<S> for SgBoxLayer
where
    S: Clone + hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + Sync + 'static,
    <S as hyper::service::Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = BoxHyperService;

    fn layer(&self, inner: S) -> Self::Service {
        self.boxed.layer(BoxHyperService::new(inner))
    }
}

impl Clone for SgBoxLayer {
    fn clone(&self) -> Self {
        Self { boxed: Arc::clone(&self.boxed) }
    }
}

impl fmt::Debug for SgBoxLayer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BoxLayer").finish()
    }
}

pub use tower::BoxError;
