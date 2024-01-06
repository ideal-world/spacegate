pub mod builder;
pub mod match_hostname;
pub mod match_request;
mod picker;
mod predicate;
use std::{convert::Infallible, num::NonZeroU16, sync::Arc, time::Duration};

use crate::{
    extension::{BackendHost, Reflect},
    utils::schema_port::port_to_schema,
    SgBody, SgBoxLayer, SgBoxService,
};

use hyper::{Request, Response};
use tower::steer::Steer;

use tower_http::timeout::{Timeout, TimeoutLayer};

use tower_layer::Layer;
use tower_service::Service;

use self::{
    builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
    match_request::SgHttpRouteMatch,
    picker::RouteByWeight,
};

/****************************************************************************************

                                          Route

*****************************************************************************************/

#[derive(Debug, Clone)]
pub struct SgHttpRoute {
    pub hostnames: Arc<[String]>,
    pub plugins: Arc<[SgBoxLayer]>,
    pub rules: Arc<[SgHttpRouteRuleLayer]>,
}

impl SgHttpRoute {
    pub fn builder() -> SgHttpRouteLayerBuilder {
        SgHttpRouteLayerBuilder::new()
    }
}
#[derive(Debug, Clone)]
pub struct SgHttpRouter {
    pub hostnames: Arc<[String]>,
    pub rules: Arc<[Arc<Option<Vec<SgHttpRouteMatch>>>]>,
}

/****************************************************************************************

                                        Route Rule

*****************************************************************************************/

#[derive(Debug, Clone)]
pub struct SgHttpRouteRuleLayer {
    pub r#match: Arc<Option<Vec<SgHttpRouteMatch>>>,
    pub plugins: Arc<[SgBoxLayer]>,
    timeouts: Option<Duration>,
    backends: Arc<[SgHttpBackendLayer]>,
}

impl SgHttpRouteRuleLayer {
    pub fn builder() -> SgHttpRouteRuleLayerBuilder {
        SgHttpRouteRuleLayerBuilder::new()
    }
}

impl<S> Layer<S> for SgHttpRouteRuleLayer
where
    S: Clone + Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + 'static,
    <S as tower_service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = SgRouteRule;

    fn layer(&self, inner: S) -> Self::Service {
        let steer = <Steer<_, _, Request<SgBody>>>::new(self.backends.iter().map(|l| l.layer(inner.clone())), RouteByWeight);
        let filter_layer = self.plugins.iter().collect::<SgBoxLayer>();
        let service = if let Some(timeout) = self.timeouts {
            SgBoxService::new(TimeoutLayer::new(timeout).layer(filter_layer.layer(steer)))
        } else {
            SgBoxService::new(filter_layer.layer(SgBoxService::new(steer)))
        };
        SgRouteRule {
            r#match: self.r#match.clone(),
            service,
        }
    }
}
#[derive(Clone)]
pub struct SgRouteRule {
    pub r#match: Arc<Option<Vec<SgHttpRouteMatch>>>,
    pub service: SgBoxService,
}

impl Service<Request<SgBody>> for SgRouteRule {
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <SgBoxService as Service<Request<SgBody>>>::Future;
    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        tracing::trace!(elapsed = ?req.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "enter route rule");
        self.service.call(req)
    }
}

/****************************************************************************************

                                        Backend

*****************************************************************************************/

#[derive(Debug, Clone)]
pub struct SgHttpBackendLayer {
    pub filters: Arc<[SgBoxLayer]>,
    pub host: Option<Arc<str>>,
    pub port: Option<NonZeroU16>,
    pub scheme: Option<Arc<str>>,
    pub weight: u16,
    pub timeout: Option<Duration>,
}

impl SgHttpBackendLayer {
    pub fn builder() -> SgHttpBackendLayerBuilder {
        SgHttpBackendLayerBuilder::new()
    }
}

impl<S> Layer<S> for SgHttpBackendLayer
where
    S: Clone + Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + 'static,
    <S as tower_service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = SgHttpBackend<SgBoxService>;

    fn layer(&self, inner: S) -> Self::Service {
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
        let service = if let Some(map_request) = map_request {
            let map_request = tower::util::MapRequestLayer::new(map_request);
            SgBoxService::new(map_request.layer(inner))
        } else {
            SgBoxService::new(inner)
        };
        let mut filtered = self.filters.iter().collect::<SgBoxLayer>().layer(service);
        if let Some(timeout) = self.timeout {
            filtered = SgBoxService::new(Timeout::new(filtered, timeout));
        }
        SgHttpBackend {
            weight: self.weight,
            inner_service: SgBoxService::new(filtered),
        }
    }
}

#[derive(Clone)]
pub struct SgHttpBackend<S> {
    pub weight: u16,
    pub inner_service: S,
}

impl<S> Service<Request<SgBody>> for SgHttpBackend<S>
where
    S: Clone + Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <S as Service<Request<SgBody>>>::Future: Send + 'static,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <SgBoxService as Service<Request<SgBody>>>::Future;

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        tracing::trace!(elapsed = ?req.extensions().get::<crate::extension::EnterTime>().map(crate::extension::EnterTime::elapsed), "enter backend");
        Box::pin(self.inner_service.call(req))
    }

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner_service.poll_ready(cx)
    }
}
