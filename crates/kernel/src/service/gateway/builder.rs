use std::{collections::HashMap, sync::Arc};

use hyper::{service::service_fn, Response};

use crate::{
    helper_layers::{function::FnLayer, reload::Reloader},
    service::http_route::HttpRoute,
    utils::Snowflake,
    ArcHyperService, BoxLayer, SgBody,
};

use super::{Gateway, HttpRouterService};

pub struct GatewayBuilder {
    pub gateway_name: Arc<str>,
    pub http_routers: HashMap<String, HttpRoute>,
    pub http_plugins: Vec<BoxLayer>,
    pub http_fallback: ArcHyperService,
    pub http_route_reloader: Reloader<HttpRouterService>,
    pub extensions: hyper::http::Extensions,
    pub x_request_id: bool,
}

/// return empty 404 not found
pub fn default_gateway_route_fallback() -> ArcHyperService {
    ArcHyperService::new(service_fn(|_| async {
        Ok(Response::builder().status(hyper::StatusCode::NOT_FOUND).body(SgBody::empty()).expect("bad response"))
    }))
}

impl GatewayBuilder {
    pub fn new(gateway_name: impl Into<Arc<str>>) -> Self {
        Self {
            gateway_name: gateway_name.into(),
            http_routers: HashMap::new(),
            http_plugins: Vec::new(),
            http_fallback: default_gateway_route_fallback(),
            http_route_reloader: Default::default(),
            extensions: hyper::http::Extensions::default(),
            x_request_id: true,
        }
    }
    pub fn x_request_id(mut self, enable: bool) -> Self {
        self.x_request_id = enable;
        self
    }
    pub fn http_router(mut self, route: HttpRoute) -> Self {
        self.http_routers.insert(route.name.clone(), route);
        self
    }
    pub fn http_routers(mut self, routes: impl IntoIterator<Item = (String, HttpRoute)>) -> Self {
        for (name, mut route) in routes {
            route.name = name.clone();
            self.http_routers.insert(name, route);
        }
        self
    }
    pub fn http_plugin(mut self, plugin: BoxLayer) -> Self {
        self.http_plugins.push(plugin);
        self
    }
    pub fn http_plugins(mut self, plugins: impl IntoIterator<Item = BoxLayer>) -> Self {
        self.http_plugins.extend(plugins);
        self
    }
    pub fn http_fallback(mut self, fallback: ArcHyperService) -> Self {
        self.http_fallback = fallback;
        self
    }
    pub fn http_route_reloader(mut self, reloader: Reloader<HttpRouterService>) -> Self {
        self.http_route_reloader = reloader;
        self
    }
    pub fn ext(mut self, extension: hyper::http::Extensions) -> Self {
        self.extensions = extension;
        self
    }
    pub fn build(self) -> Gateway {
        let mut plugins = vec![];
        if self.x_request_id {
            plugins.push(BoxLayer::new(FnLayer::new_closure(crate::utils::x_request_id::<Snowflake>)));
        }
        plugins.extend(self.http_plugins);
        Gateway {
            gateway_name: self.gateway_name,
            http_routes: self.http_routers,
            http_plugins: plugins,
            http_fallback: self.http_fallback,
            http_route_reloader: self.http_route_reloader,
            ext: self.extensions,
        }
    }
}
