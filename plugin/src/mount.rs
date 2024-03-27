use std::sync::Arc;

use serde::{Deserialize, Serialize};
use spacegate_kernel::{
    layers::{
        gateway::SgGatewayLayer,
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

impl Serialize for MountPointIndex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            MountPointIndex::Gateway { gateway } => MountPointIndexSerde::Gateway { gateway: gateway.as_ref() }.serialize(serializer),
            MountPointIndex::HttpRoute { gateway, route } => MountPointIndexSerde::HttpRoute {
                gateway: gateway.as_ref(),
                route: route.as_ref(),
            }
            .serialize(serializer),
            MountPointIndex::HttpRouteRule { gateway, route, rule } => MountPointIndexSerde::HttpRouteRule {
                gateway: gateway.as_ref(),
                route: route.as_ref(),
                rule: *rule,
            }
            .serialize(serializer),
            MountPointIndex::HttpBackend { gateway, route, rule, backend } => MountPointIndexSerde::HttpBackend {
                gateway: gateway.as_ref(),
                route: route.as_ref(),
                rule: *rule,
                backend: *backend,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for MountPointIndex {
    fn deserialize<D>(deserializer: D) -> Result<MountPointIndex, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let index = MountPointIndexSerde::deserialize(deserializer)?;
        match index {
            MountPointIndexSerde::Gateway { gateway } => Ok(MountPointIndex::Gateway { gateway: gateway.into() }),
            MountPointIndexSerde::HttpRoute { gateway, route } => Ok(MountPointIndex::HttpRoute {
                gateway: gateway.into(),
                route: route.into(),
            }),
            MountPointIndexSerde::HttpRouteRule { gateway, route, rule } => Ok(MountPointIndex::HttpRouteRule {
                gateway: gateway.into(),
                route: route.into(),
                rule,
            }),
            MountPointIndexSerde::HttpBackend { gateway, route, rule, backend } => Ok(MountPointIndex::HttpBackend {
                gateway: gateway.into(),
                route: route.into(),
                rule,
                backend,
            }),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountPointIndexSerde<'a> {
    Gateway { gateway: &'a str },
    HttpRoute { gateway: &'a str, route: &'a str },
    HttpRouteRule { gateway: &'a str, route: &'a str, rule: usize },
    HttpBackend { gateway: &'a str, route: &'a str, rule: usize, backend: usize },
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
