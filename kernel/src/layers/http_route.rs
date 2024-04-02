pub mod builder;
pub mod match_hostname;
pub mod match_request;
use std::{convert::Infallible, sync::Arc, time::Duration};

use crate::{
    extension::{BackendHost, Reflect},
    helper_layers::random_pick,
    service::ArcHyperService,
    utils::{fold_sg_layers::sg_layers, schema_port::port_to_schema},
    SgBody, SgBoxLayer,
};

use hyper::{Request, Response};

// use tower_http::timeout::{Timeout, TimeoutLayer};

use tower_layer::Layer;

use self::{
    builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
    match_request::SgHttpRouteMatch,
};

/****************************************************************************************

                                          Route

*****************************************************************************************/

#[derive(Debug)]
pub struct SgHttpRoute {
    pub name: String,
    pub hostnames: Vec<String>,
    pub plugins: Vec<SgBoxLayer>,
    pub rules: Vec<SgHttpRouteRuleLayer>,
    pub priority: i16,
    pub ext: hyper::http::Extensions,

}

impl SgHttpRoute {
    pub fn builder() -> SgHttpRouteLayerBuilder {
        SgHttpRouteLayerBuilder::new()
    }
}
#[derive(Debug, Clone)]
pub struct SgHttpRouter {
    pub hostnames: Arc<[String]>,
    pub rules: Arc<[Option<Arc<[Arc<SgHttpRouteMatch>]>>]>,
    pub ext: hyper::http::Extensions,
}

/****************************************************************************************

                                        Route Rule

*****************************************************************************************/

#[derive(Debug)]
pub struct SgHttpRouteRuleLayer {
    pub r#match: Option<Vec<SgHttpRouteMatch>>,
    pub plugins: Vec<SgBoxLayer>,
    timeouts: Option<Duration>,
    backends: Vec<SgHttpBackendLayer>,
    pub ext: hyper::http::Extensions,
}

impl SgHttpRouteRuleLayer {
    pub fn builder() -> SgHttpRouteRuleLayerBuilder {
        SgHttpRouteRuleLayerBuilder::new()
    }
}

impl<S> Layer<S> for SgHttpRouteRuleLayer
where
    S: Clone + hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + 'static,
    <S as hyper::service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = SgRouteRule;

    fn layer(&self, inner: S) -> Self::Service {
        use crate::helper_layers::timeout::TimeoutLayer;
        let empty = self.backends.is_empty();
        let filter_layer = self.plugins.iter();

        let service = if empty {
            sg_layers(filter_layer, ArcHyperService::new(TimeoutLayer::new(self.timeouts).layer(inner)))
        } else {
            let service_iter = self.backends.iter().map(|l| (l.weight, l.layer(inner.clone())));
            let random_picker = random_pick::RandomPick::new(service_iter);
            sg_layers(filter_layer, ArcHyperService::new(TimeoutLayer::new(self.timeouts).layer(random_picker)))
        };

        let r#match = self.r#match.clone().map(|v| v.into_iter().map(Arc::new).collect::<Arc<[_]>>());
        SgRouteRule {
            r#match,
            service,
            ext: self.ext.clone(),
        }
    }
}
#[derive(Clone)]
pub struct SgRouteRule {
    pub r#match: Option<Arc<[Arc<SgHttpRouteMatch>]>>,
    pub service: ArcHyperService,
    pub ext: hyper::http::Extensions,
}

impl hyper::service::Service<Request<SgBody>> for SgRouteRule {
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <ArcHyperService as hyper::service::Service<Request<SgBody>>>::Future;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        tracing::trace!(elapsed = ?req.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "enter route rule");
        let fut = self.service.call(req);
        Box::pin(async move {
            let result = fut.await;
            tracing::trace!("finished route rule");
            result
        })
    }
}

/****************************************************************************************

                                        Backend

*****************************************************************************************/

#[derive(Debug)]
pub struct SgHttpBackendLayer {
    pub plugins: Vec<SgBoxLayer>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub scheme: Option<String>,
    pub weight: u16,
    pub timeout: Option<Duration>,
    pub ext: hyper::http::Extensions,
}

impl SgHttpBackendLayer {
    pub fn builder() -> SgHttpBackendLayerBuilder {
        SgHttpBackendLayerBuilder::new()
    }
}

impl<S> Layer<S> for SgHttpBackendLayer
where
    S: Clone + hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + 'static,
    <S as hyper::service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = SgHttpBackend<ArcHyperService>;

    fn layer(&self, inner: S) -> Self::Service {
        let timeout_layer = crate::helper_layers::timeout::TimeoutLayer::new(self.timeout);
        let filtered = sg_layers(self.plugins.iter(), ArcHyperService::new(timeout_layer.layer(inner)));
        SgHttpBackend {
            weight: self.weight,
            host: self.host.clone().map(Into::into),
            port: self.port,
            scheme: self.scheme.clone().map(Into::into),
            timeout: self.timeout,
            inner_service: filtered,
            ext: self.ext.clone(),
        }
    }
}

#[derive(Clone)]
pub struct SgHttpBackend<S> {
    pub host: Option<Arc<str>>,
    pub port: Option<u16>,
    pub scheme: Option<Arc<str>>,
    pub weight: u16,
    pub timeout: Option<Duration>,
    pub inner_service: S,
    pub ext: hyper::http::Extensions,
}

impl<S> hyper::service::Service<Request<SgBody>> for SgHttpBackend<S>
where
    S: Clone + hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <S as hyper::service::Service<Request<SgBody>>>::Future: Send + 'static,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = S::Future;

    fn call(&self, req: Request<SgBody>) -> Self::Future {
        let map_request = match (self.host.clone(), self.port, self.scheme.clone()) {
            (None, None, None) => None,
            (host, port, schema) => Some(move |mut req: Request<SgBody>| {
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
                if let Ok(uri) = builder.build() {
                    tracing::trace!("[Sg.Backend] new uri: {uri}");
                    *req.uri_mut() = uri;
                }
                req
            }),
        };
        tracing::trace!(elapsed = ?req.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "enter backend");
        let req = if let Some(map_request) = map_request { map_request(req) } else { req };
        self.inner_service.call(req)
    }
}
