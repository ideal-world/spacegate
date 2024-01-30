use serde::{Deserialize, Serialize};
use serde_json::Value;
use spacegate_plugin::MakeSgLayer;
use spacegate_tower::{
    layers::{
        gateway::builder::SgGatewayLayerBuilder,
        http_route::builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
    },
    BoxError, SgBoxLayer,
};
use tardis::log;

/// RouteFilter defines processing steps that must be completed during the request or response lifecycle.
///
/// There are four levels of filters
/// 1. Global level, which works on all requests under the same gateway service
/// 2. Routing level, which works on all requests under the same gateway route
/// 3. Rule level, which works on all requests under the same gateway routing rule
/// 4. Backend level, which works on all requests under the same backend
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgRouteFilter {
    /// Filter code, Used to match the corresponding filter.
    pub code: String,
    /// Filter name, the name of the same filter exists at different levels of configuration, only the child nodes take effect（Backend Level > Rule Level > Routing Level > Global Level）
    pub name: Option<String>,
    /// filter parameters.
    pub spec: Value,
}

impl SgRouteFilter {
    pub fn into_layer(self) -> Result<SgBoxLayer, BoxError> {
        let plugin_repo = spacegate_plugin::SgPluginRepository::global();
        plugin_repo.create(&self.code, self.spec)?.make_layer()
    }
    pub fn create(self) -> Result<Box<dyn MakeSgLayer>, BoxError> {
        let plugin_repo = spacegate_plugin::SgPluginRepository::global();
        plugin_repo.create(&self.code, self.spec)
    }
    pub fn install_on_backend(iter: impl IntoIterator<Item = Self>, mut builder: SgHttpBackendLayerBuilder) -> SgHttpBackendLayerBuilder {
        for filter in iter {
            if let Err(e) = filter.create().and_then(|layer| layer.install_on_backend(&mut builder)) {
                log::error!("[Sg.Plugins] install_on_backend error: {}", e);
            }
        }
        builder
    }
    pub fn install_on_gateway(iter: impl IntoIterator<Item = Self>, mut builder: SgGatewayLayerBuilder) -> SgGatewayLayerBuilder {
        for filter in iter {
            if let Err(e) = filter.create().and_then(|layer| layer.install_on_gateway(&mut builder)) {
                log::error!("[Sg.Plugins] install_on_gateway error: {}", e);
            }
        }
        builder
    }
    pub fn install_on_rule(iter: impl IntoIterator<Item = Self>, mut builder: SgHttpRouteRuleLayerBuilder) -> SgHttpRouteRuleLayerBuilder {
        for filter in iter {
            if let Err(e) = filter.create().and_then(|layer| layer.install_on_rule(&mut builder)) {
                log::error!("[Sg.Plugins] install_on_rule error: {}", e);
            }
        }
        builder
    }
    pub fn install_on_route(iter: impl IntoIterator<Item = Self>, mut builder: SgHttpRouteLayerBuilder) -> SgHttpRouteLayerBuilder {
        for filter in iter {
            if let Err(e) = filter.create().and_then(|layer| layer.install_on_route(&mut builder)) {
                log::error!("[Sg.Plugins] install_on_route error: {}", e);
            }
        }
        builder
    }
}
