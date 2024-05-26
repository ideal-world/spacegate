pub mod builder;
pub mod match_hostname;
pub mod match_request;
use std::{convert::Infallible, path::PathBuf, sync::Arc, time::Duration};
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
use crate::{
    backend_service::{get_http_backend_service, http_backend_service, static_file_service::static_file_service, ArcHyperService},
    extension::{BackendHost, Reflect},
    helper_layers::balancer::{self, Balancer},
    utils::{fold_box_layers::fold_layers, schema_port::port_to_schema},
    BoxLayer, SgBody,
};

use futures_util::future::BoxFuture;
use hyper::{Request, Response};

use tower_layer::Layer;

use self::{
    builder::{HttpBackendBuilder, HttpRouteBuilder, HttpRouteRuleBuilder},
    match_request::HttpRouteMatch,
};

/****************************************************************************************

                                          Route

*****************************************************************************************/

#[derive(Debug)]
pub struct HttpRoute {
    pub name: String,
    pub hostnames: Vec<String>,
    pub plugins: Vec<BoxLayer>,
    pub rules: Vec<HttpRouteRule>,
    pub priority: i16,
    pub ext: hyper::http::Extensions,
}

impl HttpRoute {
    pub fn builder() -> HttpRouteBuilder {
        HttpRouteBuilder::new()
    }
}
#[derive(Debug, Clone)]
pub struct HttpRouter {
    pub hostnames: Arc<[String]>,
    pub rules: Arc<[Option<Arc<[Arc<HttpRouteMatch>]>>]>,
    pub ext: hyper::http::Extensions,
}

/****************************************************************************************

                                        Route Rule

*****************************************************************************************/

#[derive(Debug)]
pub struct HttpRouteRule {
    pub r#match: Option<Vec<HttpRouteMatch>>,
    pub plugins: Vec<BoxLayer>,
    timeouts: Option<Duration>,
    backends: Vec<HttpBackend>,
    balance_policy: BalancePolicyEnum,
    pub ext: hyper::http::Extensions,
}

#[derive(Debug, Default)]
pub enum BalancePolicyEnum {
    Random,
    #[default]
    IpHash,
}

impl HttpRouteRule {
    pub fn builder() -> HttpRouteRuleBuilder {
        HttpRouteRuleBuilder::new()
    }
    pub fn as_service(&self) -> HttpRouteRuleService {
        use crate::helper_layers::timeout::TimeoutLayer;
        let filter_layer = self.plugins.iter();
        let time_out = self.timeouts.unwrap_or(DEFAULT_TIMEOUT);
        let fallback = get_http_backend_service();
        let service_iter = self.backends.iter().map(HttpBackend::as_service).collect::<Vec<_>>();
        let balanced = match self.balance_policy {
            BalancePolicyEnum::Random => {
                let weights = self.backends.iter().map(|x| x.weight);
                ArcHyperService::new(Balancer::new(balancer::Random::new(weights), service_iter, fallback))
            }
            BalancePolicyEnum::IpHash => ArcHyperService::new(Balancer::new(balancer::IpHash::default(), service_iter, fallback)),
        };
        let service = fold_layers(filter_layer, ArcHyperService::new(TimeoutLayer::new(time_out).layer(balanced)));
        HttpRouteRuleService { service }
    }
}

#[derive(Clone)]
pub struct HttpRouteRuleService {
    pub service: ArcHyperService,
}

impl hyper::service::Service<Request<SgBody>> for HttpRouteRuleService {
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <ArcHyperService as hyper::service::Service<Request<SgBody>>>::Future;
    // #[instrument("route_rule", skip_all)]
    fn call(&self, req: Request<SgBody>) -> Self::Future {
        let fut = self.service.call(req);
        Box::pin(fut)
    }
}

/****************************************************************************************

                                        Backend

*****************************************************************************************/
#[derive(Debug)]
pub struct HttpBackend {
    pub plugins: Vec<BoxLayer>,
    pub backend: Backend,
    pub weight: u16,
    pub timeout: Option<Duration>,
    pub ext: hyper::http::Extensions,
}

impl HttpBackend {
    pub fn builder() -> HttpBackendBuilder {
        HttpBackendBuilder::new()
    }
    pub fn as_service(&self) -> ArcHyperService {
        let inner_service = HttpBackendService {
            backend: self.backend.clone().into(),
        };
        let timeout_layer = crate::helper_layers::timeout::TimeoutLayer::new(self.timeout.unwrap_or(DEFAULT_TIMEOUT));
        let filtered = fold_layers(self.plugins.iter(), ArcHyperService::new(timeout_layer.layer(inner_service)));
        filtered
    }
}

#[derive(Clone, Debug)]
pub enum Backend {
    Http { host: Option<String>, port: Option<u16>, schema: Option<String> },
    File { path: PathBuf },
}

#[derive(Clone)]
pub struct HttpBackendService {
    pub backend: Arc<Backend>,
}

impl HttpBackendService {
    pub fn http_default() -> Self {
        Self {
            backend: Arc::new(Backend::Http {
                host: None,
                port: None,
                schema: None,
            }),
        }
    }
}

impl hyper::service::Service<Request<SgBody>> for HttpBackendService {
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Response<SgBody>, Infallible>>;

    fn call(&self, mut req: Request<SgBody>) -> Self::Future {
        let req = match self.backend.as_ref() {
            Backend::Http {
                host: None,
                port: None,
                schema: None,
            }
            | Backend::File { .. } => req,
            Backend::Http { host, port, schema } => {
                if let Some(ref host) = host {
                    if let Some(reflect) = req.extensions_mut().get_mut::<Reflect>() {
                        reflect.insert(BackendHost::new(host.clone()));
                    }
                    req.extensions_mut().insert(BackendHost::new(host.clone()));
                }
                let uri = req.uri_mut();
                let (raw_host, raw_port) = if let Some(auth) = uri.authority() { (auth.host(), auth.port_u16()) } else { ("", None) };
                let new_host = host.as_deref().unwrap_or(raw_host);
                let new_port = port.map(u16::from).or(raw_port);
                let new_scheme = schema.as_deref().or(uri.scheme_str()).or_else(|| new_port.and_then(port_to_schema)).unwrap_or("http");
                let mut builder = hyper::http::uri::Uri::builder().scheme(new_scheme);
                if let Some(new_port) = new_port {
                    builder = builder.authority(format!("{}:{}", new_host, new_port));
                } else {
                    builder = builder.authority(new_host);
                };
                if let Some(path_and_query) = uri.path_and_query() {
                    builder = builder.path_and_query(path_and_query.clone());
                }
                match builder.build() {
                    Ok(uri) => {
                        tracing::trace!("[Sg.Backend] new uri: {uri}");
                        *req.uri_mut() = uri;
                    }
                    Err(e) => {
                        tracing::error!("Failed to build uri: {}", e);
                    }
                }
                req
            }
        };
        let backend = self.backend.clone();
        tracing::trace!(elapsed = ?req.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "enter backend {backend:?}");
        Box::pin(async move {
            unsafe {
                let mut response = match backend.as_ref() {
                    Backend::Http { .. } => http_backend_service(req).await.unwrap_unchecked(),
                    Backend::File { path } => static_file_service(req, path).await,
                };
                response.extensions_mut().insert(crate::extension::FromBackend::new());
                tracing::trace!(elapsed = ?response.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "finish backend request");
                Ok(response)
            }
        })
    }
}
