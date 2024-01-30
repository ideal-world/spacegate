use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(feature = "service")]
pub mod service;

pub mod model;
use model::gateway::SgGateway;
use model::http_route::SgHttpRoute;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
pub struct ConfigItem {
    pub gateway: SgGateway,
    pub routes: BTreeMap<String, SgHttpRoute>,
}

impl ConfigItem {
    pub fn into_gateway_and_routes(self) -> (SgGateway, Vec<SgHttpRoute>) {
        (self.gateway, self.routes.into_values().collect())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
#[serde(transparent)]
pub struct Config {
    pub gateways: BTreeMap<String, ConfigItem>,
}
