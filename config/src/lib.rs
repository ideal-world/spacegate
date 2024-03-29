use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod constants;
#[cfg(feature = "k8s")]
pub mod k8s_crd;
pub mod service;

pub mod model;
use model::gateway::SgGateway;
use model::http_route::SgHttpRoute;

type BoxResult<T> = Result<T, BoxError>;
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
pub struct ConfigItem {
    pub gateway: SgGateway,
    pub routes: BTreeMap<String, SgHttpRoute>,
}

impl ConfigItem {
    pub fn collect_all_plugins(&self) -> Vec<crate::model::PluginConfig> {
        let mut plugins = Vec::new();
        plugins.extend(self.gateway.filters.iter().cloned());
        for route in self.routes.values() {
            plugins.extend(route.filters.iter().cloned());
            for rule in route.rules.iter() {
                plugins.extend(rule.filters.iter().cloned());
                for backend in rule.backends.iter() {
                    plugins.extend(backend.filters.iter().cloned());
                }
            }
        }
        plugins
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
#[serde(transparent)]
pub struct Config {
    pub gateways: BTreeMap<String, ConfigItem>,
}
