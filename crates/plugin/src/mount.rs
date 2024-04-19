use std::sync::Arc;

use serde::{Deserialize, Serialize};
use spacegate_kernel::{
    layers::{
        gateway::Gateway,
        http_route::{HttpBackend, HttpRoute, HttpRouteRule},
    },
    BoxError,
};

use crate::instance::{drop_trace, DropMarkerSet, DropTracer, PluginInstance};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum MountPointIndex {
    Gateway { gateway: Arc<str> },
    HttpRoute { gateway: Arc<str>, route: Arc<str> },
    HttpRouteRule { gateway: Arc<str>, route: Arc<str>, rule: usize },
    HttpBackend { gateway: Arc<str>, route: Arc<str>, rule: usize, backend: usize },
}

impl MountPointIndex {
    pub fn gateway(&self) -> &str {
        match self {
            MountPointIndex::Gateway { gateway } => gateway.as_ref(),
            MountPointIndex::HttpRoute { gateway, .. } => gateway.as_ref(),
            MountPointIndex::HttpRouteRule { gateway, .. } => gateway.as_ref(),
            MountPointIndex::HttpBackend { gateway, .. } => gateway.as_ref(),
        }
    }
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
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<DropTracer, BoxError>;
}

impl MountPoint for Gateway {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<DropTracer, BoxError> {
        let (tracer, marker) = drop_trace();
        self.http_plugins.push(instance.make());
        let set = self.ext.get_or_insert_default::<DropMarkerSet>();
        set.inner.insert(marker);
        Ok(tracer)
    }
}

impl MountPoint for HttpRoute {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<DropTracer, BoxError> {
        let (tracer, marker) = drop_trace();
        self.plugins.push(instance.make());
        let set = self.ext.get_or_insert_default::<DropMarkerSet>();
        set.inner.insert(marker);
        Ok(tracer)
    }
}

impl MountPoint for HttpRouteRule {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<DropTracer, BoxError> {
        let (tracer, marker) = drop_trace();
        self.plugins.push(instance.make());
        let set = self.ext.get_or_insert_default::<DropMarkerSet>();
        set.inner.insert(marker);
        Ok(tracer)
    }
}

impl MountPoint for HttpBackend {
    fn mount(&mut self, instance: &mut PluginInstance) -> Result<DropTracer, BoxError> {
        let (tracer, marker) = drop_trace();
        self.plugins.push(instance.make());
        let set = self.ext.get_or_insert_default::<DropMarkerSet>();
        set.inner.insert(marker);
        Ok(tracer)
    }
}
