use std::sync::Arc;

use spacegate_kernel::{
    layers::{
        gateway::{SgGatewayLayer, SgGatewayRoute, SgGatewayRouter},
        http_route::{SgHttpBackendLayer, SgHttpRoute, SgHttpRouteRuleLayer},
    },
    BoxError,
};

use crate::instance::PluginInstance;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum MountPointIndex {
    Gateway { gateway: Arc<str> },
    HttpRoute { gateway: Arc<str>, route: Arc<str> },
    HttpRouteRule { gateway: Arc<str>, route: Arc<str>, rule: usize },
    HttpBackend { gateway: Arc<str>, route: Arc<str>, rule: usize, backend: usize },
}

pub trait MountPoint {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<(), BoxError>;
}

impl MountPoint for SgGatewayLayer {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<(), BoxError> {
        self.http_plugins.push(instance.make()?);
        Ok(())
    }
}

impl MountPoint for SgHttpRoute {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<(), BoxError> {
        self.plugins.push(instance.make()?);
        Ok(())
    }
}

impl MountPoint for SgHttpRouteRuleLayer {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<(), BoxError> {
        self.plugins.push(instance.make()?);
        Ok(())
    }
}

impl MountPoint for SgHttpBackendLayer {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<(), BoxError> {
        self.plugins.push(instance.make()?);
        Ok(())
    }
}
