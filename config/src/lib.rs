use std::collections::BTreeMap;

use model::gateway::SgGateway;
use model::http_route::SgHttpRoute;
use serde::{Deserialize, Serialize};
pub mod backend;
pub mod config_format;
pub mod constants;
pub mod create;
pub mod delete;
pub mod k8s_crd;
pub mod model;
pub mod retrieve;
pub mod update;

#[derive(Default, Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
pub struct ConfigItem {
    pub gateway: SgGateway,
    pub routes: BTreeMap<String, SgHttpRoute>,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, schemars::JsonSchema)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
#[serde(transparent)]
pub struct Config {
    pub gateways: BTreeMap<String, ConfigItem>,
}
