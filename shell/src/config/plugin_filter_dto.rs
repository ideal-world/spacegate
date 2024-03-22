use spacegate_config::model::SgRouteFilter;
use spacegate_kernel::{
    layers::{
        gateway::builder::SgGatewayLayerBuilder,
        http_route::builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
    },
    BoxError, SgBoxLayer,
};
use spacegate_plugin::MakeSgLayer;

/// Extension trait for [`SgRouteFilter`] to install on backend, gateway, rule and route in a more convenient way.
pub trait FilterInstallExt: Sized {
    fn into_layer(self) -> Result<SgBoxLayer, BoxError>;
    fn create(self) -> Result<Box<dyn MakeSgLayer>, BoxError>;
    fn install_on_backend(iter: impl IntoIterator<Item = Self>, mut builder: SgHttpBackendLayerBuilder) -> SgHttpBackendLayerBuilder {
        for filter in iter {
            if let Err(e) = filter.create().and_then(|layer| layer.install_on_backend(&mut builder)) {
                tracing::error!("[Sg.Plugins] install_on_backend error: {}", e);
            }
        }
        builder
    }
    fn install_on_gateway(iter: impl IntoIterator<Item = Self>, mut builder: SgGatewayLayerBuilder) -> SgGatewayLayerBuilder {
        for filter in iter {
            if let Err(e) = filter.create().and_then(|layer| layer.install_on_gateway(&mut builder)) {
                tracing::error!("[Sg.Plugins] install_on_gateway error: {}", e);
            }
        }
        builder
    }
    fn install_on_rule(iter: impl IntoIterator<Item = Self>, mut builder: SgHttpRouteRuleLayerBuilder) -> SgHttpRouteRuleLayerBuilder {
        for filter in iter {
            if let Err(e) = filter.create().and_then(|layer| layer.install_on_rule(&mut builder)) {
                tracing::error!("[Sg.Plugins] install_on_rule error: {}", e);
            }
        }
        builder
    }
    fn install_on_route(iter: impl IntoIterator<Item = Self>, mut builder: SgHttpRouteLayerBuilder) -> SgHttpRouteLayerBuilder {
        for filter in iter {
            if let Err(e) = filter.create().and_then(|layer| layer.install_on_route(&mut builder)) {
                tracing::error!("[Sg.Plugins] install_on_route error: {}", e);
            }
        }
        builder
    }
}

impl FilterInstallExt for SgRouteFilter {
    fn into_layer(self) -> Result<SgBoxLayer, BoxError> {
        let plugin_repo = spacegate_plugin::SgPluginRepository::global();
        plugin_repo.create(self.name, &self.code, self.spec)?.make_layer()
    }
    fn create(self) -> Result<Box<dyn MakeSgLayer>, BoxError> {
        let plugin_repo = spacegate_plugin::SgPluginRepository::global();
        plugin_repo.create(self.name, &self.code, self.spec)
    }
}
