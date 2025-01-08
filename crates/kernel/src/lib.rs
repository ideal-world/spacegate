//! # Spacegate kernel crate.
//!
//! This crate provides the core functionality of spacegate.

#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo, clippy::missing_safety_doc)]
#![warn(
    clippy::missing_errors_doc,
    clippy::indexing_slicing,
    clippy::inline_always,
    clippy::fn_params_excessive_bools,
    missing_debug_implementations
)]
/// https services, ws services, and static file services.
pub mod backend_service;
/// a boxed body
pub mod body;
/// extensions for request and response
pub mod extension;
/// extractors for request
pub mod extractor;

/// helper layers
pub mod helper_layers;
/// injectors for reqeust
pub mod injector;
/// tcp listener
pub mod listener;
/// gateway service
pub mod service;
/// util functions and structs
pub mod utils;

pub use backend_service::ArcHyperService;
pub use body::SgBody;
use extension::Reflect;
pub use extractor::Extract;
use extractor::OptionalExtract;
use hyper::{body::Bytes, Request, Response, StatusCode};
use injector::Inject;
use std::{convert::Infallible, fmt, ops::Deref};
pub use tokio_util::sync::CancellationToken;
pub use tower_layer::Layer;
use utils::{PathIter, QueryKvIter};

use tower_layer::layer_fn;

pub type BoxResult<T> = Result<T, BoxError>;
/// A boxed error.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Alias for a request with a boxed body.
pub type SgRequest = Request<SgBody>;
/// Alias for a response with a boxed body.
pub type SgResponse = Response<SgBody>;

/// Provides extension methods for [`Request`](hyper::Request).
pub trait SgRequestExt {
    fn with_reflect(&mut self);
    fn reflect_mut(&mut self) -> &mut Reflect;
    fn reflect(&self) -> &Reflect;
    #[cfg(feature = "ext-redis")]
    fn get_redis_client_by_gateway_name(&self) -> Option<spacegate_ext_redis::RedisClient>;
    fn extract<M: Extract>(&self) -> M;
    fn try_extract<M: OptionalExtract>(&self) -> Option<M>;
    /// # Errors
    /// If the injection fails.
    fn inject<I: Inject>(&mut self, i: &I) -> BoxResult<()>;
    fn defer_call<F>(&mut self, f: F)
    where
        F: FnOnce(SgRequest) -> SgRequest + Send + 'static;
    fn path_iter(&self) -> PathIter;
    fn query_kv_iter(&self) -> Option<QueryKvIter>;
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
    /// Get a redis client by the [`extension::GatewayName`], which would exist once the request had entered some gateway.
    fn get_redis_client_by_gateway_name(&self) -> Option<spacegate_ext_redis::RedisClient> {
        self.extensions().get::<extension::GatewayName>().and_then(|gateway_name| spacegate_ext_redis::RedisClientRepo::global().get(gateway_name))
    }

    /// Extract a value from the request.
    fn extract<M: Extract>(&self) -> M {
        M::extract(self)
    }

    /// Try to extract a value from the request.
    fn try_extract<M: OptionalExtract>(&self) -> Option<M> {
        OptionalExtract::extract(self)
    }

    /// Inject some data into the request.
    fn inject<I: Inject>(&mut self, i: &I) -> BoxResult<()> {
        i.inject(self)
    }

    /// Defer a call to the request. The call will be executed before the request has been sent to the backend.
    fn defer_call<F>(&mut self, f: F)
    where
        F: FnOnce(SgRequest) -> SgRequest + Send + 'static,
    {
        let defer = self.extensions_mut().get_or_insert_default::<extension::Defer>();
        defer.push_back(f);
    }

    fn path_iter(&self) -> PathIter {
        PathIter::new(self.uri().path())
    }

    fn query_kv_iter(&self) -> Option<QueryKvIter> {
        self.uri().query().map(QueryKvIter::new)
    }
}

/// Provides extension methods for [`Response`](hyper::Response).
pub trait SgResponseExt {
    fn with_code_message(code: StatusCode, message: impl Into<Bytes>) -> Self;
    fn with_code_empty(code: StatusCode) -> Self;
    fn bad_gateway<E: std::error::Error>(e: E) -> Self
    where
        Self: Sized,
    {
        let message = e.to_string();
        let src = e.source();
        let message = if let Some(src) = src { format!("{}:\n {}", message, src) } else { message };
        Self::with_code_message(StatusCode::BAD_GATEWAY, message)
    }
    fn inherit_reflect(&mut self, req: &SgRequest);
}

impl SgResponseExt for Response<SgBody> {
    fn with_code_message(code: StatusCode, message: impl Into<Bytes>) -> Self {
        let body = SgBody::full(message);
        let mut resp = Response::builder().status(code).body(body).expect("response builder error");
        resp.extensions_mut().insert(Reflect::new());
        resp
    }
    fn with_code_empty(code: StatusCode) -> Self {
        let body = SgBody::empty();
        let mut resp = Response::builder().status(code).body(body).expect("response builder error");
        resp.extensions_mut().insert(Reflect::new());
        resp
    }
    fn inherit_reflect(&mut self, req: &SgRequest) {
        if let Some(reflect) = req.extensions().get::<Reflect>() {
            self.extensions_mut().extend(reflect.deref().clone());
        }
    }
}

/// A boxed [`Layer`] that can be used as a plugin layer in gateway.
pub struct BoxLayer {
    boxed: Box<dyn Layer<ArcHyperService, Service = ArcHyperService> + Send + Sync + 'static>,
}

impl BoxLayer {
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

    /// Create a new [`BoxLayer`] with an arc wrapped layer.
    #[must_use]
    pub fn layer_shared(&self, inner: ArcHyperService) -> ArcHyperService {
        self.boxed.layer(inner)
    }
}

impl<S> Layer<S> for BoxLayer
where
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + Sync + 'static,
    <S as hyper::service::Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = ArcHyperService;

    fn layer(&self, inner: S) -> Self::Service {
        self.boxed.layer(ArcHyperService::new(inner))
    }
}

impl fmt::Debug for BoxLayer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BoxLayer").finish()
    }
}
