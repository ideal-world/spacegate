use std::collections::BTreeMap;

use model::gateway::SgGateway;
use model::http_route::SgHttpRoute;
use serde::{Deserialize, Serialize};
pub mod backend;
pub mod config_format;
pub mod model;
pub mod retrieve;
pub mod create;
pub mod update;
pub mod delete;


#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct ConfigItem {
    pub gateway: SgGateway,
    pub routes: BTreeMap<String, SgHttpRoute>,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(transparent)]
pub struct Config {
    pub gateways: BTreeMap<String, ConfigItem>,
}
