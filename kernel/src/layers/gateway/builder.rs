use std::{collections::HashMap, sync::Arc};

use crate::{
    helper_layers::{
        filter::{response_anyway::ResponseAnyway, FilterRequestLayer},
        reload::Reloader,
    },
    layers::http_route::SgHttpRoute,
    SgBoxLayer,
};

use super::{SgGatewayLayer, SgGatewayRoute};

pub struct SgGatewayLayerBuilder {
    pub gateway_name: Arc<str>,
    http_routers: HashMap<String, SgHttpRoute>,
    pub http_plugins: Vec<SgBoxLayer>,
    http_fallback: SgBoxLayer,
    http_route_reloader: Reloader<SgGatewayRoute>,
    pub extension: hyper::http::Extensions,
}

pub fn default_gateway_route_fallback() -> SgBoxLayer {
    // static LAYER: OnceLock<SgBoxLayer> = OnceLock::new();
    // LAYER.get_or_init(|| {
    // })
    SgBoxLayer::new(FilterRequestLayer::new(ResponseAnyway {
        status: hyper::StatusCode::NOT_FOUND,
        message: "[Sg.HttpRouteRule] no rule matched".to_string().into(),
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
            extension: hyper::http::Extensions::default(),
        }
    }
    pub fn http_router(mut self, route: SgHttpRoute) -> Self {
        self.http_routers.insert(route.name.clone(), route);
        self
    }
    pub fn http_routers(mut self, routes: impl IntoIterator<Item = (String, SgHttpRoute)>) -> Self {
        for (name, mut route) in routes {
            route.name = name.clone();
            self.http_routers.insert(name, route);
        }
        self
    }
    pub fn http_plugin(mut self, plugin: SgBoxLayer) -> Self {
        self.http_plugins.push(plugin);
        self
    }
    pub fn http_plugins(mut self, plugins: impl IntoIterator<Item = SgBoxLayer>) -> Self {
        self.http_plugins.extend(plugins);
        self
    }
    pub fn http_fallback(mut self, fallback: SgBoxLayer) -> Self {
        self.http_fallback = fallback;
        self
    }
    pub fn http_route_reloader(mut self, reloader: Reloader<SgGatewayRoute>) -> Self {
        self.http_route_reloader = reloader;
        self
    }
    pub fn ext(mut self, extension: hyper::http::Extensions) -> Self {
        self.extension = extension;
        self
    }
    pub fn build(self) -> SgGatewayLayer {
        SgGatewayLayer {
            gateway_name: self.gateway_name,
            http_routes: self.http_routers,
            http_plugins: self.http_plugins,
            http_fallback: self.http_fallback,
            http_route_reloader: self.http_route_reloader,
        }
    }
}
