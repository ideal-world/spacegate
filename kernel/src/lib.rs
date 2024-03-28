#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo)]

// pub mod config;
pub mod body;
pub mod extension;
pub mod extractor;
pub mod header;
pub mod helper_layers;
pub mod layers;
pub mod listener;
pub mod service;
pub mod utils;

pub use body::SgBody;
use extension::Reflect;
pub use extractor::Extractor;
use helper_layers::response_error::ErrorFormatter;
pub use service::ArcHyperService;
use std::{convert::Infallible, fmt};
pub use tower_layer::Layer;

use hyper::{body::Bytes, Request, Response, StatusCode};

use tower_layer::layer_fn;

pub type BoxResult<T> = Result<T, BoxError>;
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type SgRequest = Request<SgBody>;
pub type SgResponse = Response<SgBody>;

pub trait SgRequestExt {
    fn with_reflect(&mut self);
    fn reflect_mut(&mut self) -> &mut Reflect;
    fn reflect(&self) -> &Reflect;
    #[cfg(feature = "ext-redis")]
    fn get_redis_client_by_gateway_name(&self) -> Option<spacegate_ext_redis::RedisClient>;
    fn extract_marker<M: Extractor>(&self) -> Option<M>;
}

impl SgRequestExt for SgRequest {
    /// Get a mutable reference to the reflect extension.
    ///
    /// # Panics
    /// Panics if the reflect extension is not found.
    /// If you are using a request created by spacegate, this should never happen.
    fn reflect_mut(&mut self) -> &mut Reflect {
        self.extensions_mut().get_mut::<Reflect>().expect("reflect extension not found")
    }
    /// Get a reference to the reflect extension.
    ///
    /// # Panics
    /// Panics if the reflect extension is not found.
    /// If you are using a request created by spacegate, this should never happen.
    fn reflect(&self) -> &Reflect {
        self.extensions().get::<Reflect>().expect("reflect extension not found")
    }
    /// Add a reflect extension to the request if it does not exist.
    fn with_reflect(&mut self) {
        if self.extensions().get::<Reflect>().is_none() {
            self.extensions_mut().insert(Reflect::new());
        }
    }

    #[cfg(feature = "ext-redis")]
    fn get_redis_client_by_gateway_name(&self) -> Option<spacegate_ext_redis::RedisClient> {
        self.extensions().get::<extension::GatewayName>().and_then(|gateway_name| spacegate_ext_redis::RedisClientRepo::global().get(gateway_name))
    }

    fn extract_marker<M: Extractor>(&self) -> Option<M> {
        M::extract(self)
    }
}

pub trait SgResponseExt {
    fn with_code_message(code: StatusCode, message: impl Into<Bytes>) -> Self;
    fn bad_gateway<E: std::error::Error>(e: E) -> Self
    where
        Self: Sized,
    {
        let message = e.to_string();
        Self::with_code_message(StatusCode::BAD_GATEWAY, message)
    }
    fn from_error<E: std::error::Error, F: ErrorFormatter>(e: E, formatter: &F) -> Self
    where
        Self: Sized,
    {
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
    boxed: Box<dyn Layer<ArcHyperService, Service = ArcHyperService> + Send + Sync + 'static>,
}

// impl<'a> FromIterator<&'a SgBoxLayer> for SgBoxLayer {
//     fn from_iter<T: IntoIterator<Item = &'a SgBoxLayer>>(iter: T) -> Self {
//         fold_sg_layers(iter.into_iter().cloned())
//     }
// }

impl SgBoxLayer {
    /// Create a new [`BoxLayer`].
    pub fn new<L>(inner_layer: L) -> Self
    where
        L: Layer<ArcHyperService> + Send + Sync + 'static,
        L::Service: Clone + hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + Sync + 'static,
        <L::Service as hyper::service::Service<Request<SgBody>>>::Future: Send + 'static,
    {
        let layer = layer_fn(move |inner: ArcHyperService| {
            let out = inner_layer.layer(inner);
            ArcHyperService::new(out)
        });

        Self { boxed: Box::new(layer) }
    }
    pub fn layer_boxed(&self, inner: ArcHyperService) -> ArcHyperService {
        self.boxed.layer(inner)
    }
}

impl<S> Layer<S> for SgBoxLayer
where
    S: Clone + hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + Sync + 'static,
    <S as hyper::service::Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = ArcHyperService;

    fn layer(&self, inner: S) -> Self::Service {
        self.boxed.layer(ArcHyperService::new(inner))
    }
}

impl fmt::Debug for SgBoxLayer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BoxLayer").finish()
    }
}
