use std::{collections::HashMap, sync::Arc};

use hyper::{service::service_fn, Response};

use crate::{
    helper_layers::{function::FnLayer, reload::Reloader},
    layers::http_route::HttpRoute,
    utils::Snowflake,
    ArcHyperService, BoxLayer, SgBody,
};

use super::{SgGatewayLayer, SgGatewayRoute};

pub struct SgGatewayLayerBuilder {
    pub gateway_name: Arc<str>,
    pub http_routers: HashMap<String, HttpRoute>,
    pub http_plugins: Vec<BoxLayer>,
    pub http_fallback: ArcHyperService,
    pub http_route_reloader: Reloader<SgGatewayRoute>,
    pub extensions: hyper::http::Extensions,
    pub x_request_id: bool,
}

pub fn default_gateway_route_fallback() -> ArcHyperService {
    // static LAYER: OnceLock<SgBoxLayer> = OnceLock::new();
    // LAYER.get_or_init(|| {
    // })
    ArcHyperService::new(service_fn(|_| async {
        Ok(Response::builder().status(hyper::StatusCode::NOT_FOUND).body(SgBody::full("[Sg.HttpRouteRule] no rule matched")).expect("bad response"))
    }))
}

impl SgGatewayLayerBuilder {
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
    pub fn http_route_reloader(mut self, reloader: Reloader<SgGatewayRoute>) -> Self {
        self.http_route_reloader = reloader;
        self
    }
    pub fn ext(mut self, extension: hyper::http::Extensions) -> Self {
        self.extensions = extension;
        self
    }
    pub fn build(self) -> SgGatewayLayer {
        let mut plugins = vec![];
        if self.x_request_id {
            plugins.push(BoxLayer::new(FnLayer::new_closure(crate::utils::x_request_id::<Snowflake>)));
        }
        plugins.extend(self.http_plugins);
        SgGatewayLayer {
            gateway_name: self.gateway_name,
            http_routes: self.http_routers,
            http_plugins: plugins,
            http_fallback: self.http_fallback,
            http_route_reloader: self.http_route_reloader,
            ext: self.extensions,
        }
    }
}
