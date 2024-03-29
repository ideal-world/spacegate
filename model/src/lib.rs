pub mod plugin;
use std::collections::BTreeMap;

pub use plugin::*;

pub mod gateway;
pub use gateway::*;

pub mod http_route;
pub use http_route::*;
use serde::{Deserialize, Serialize};

pub mod route_match;

pub mod constants;
pub mod ext;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
pub type BoxResult<T> = Result<T, BoxError>;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
pub struct ConfigItem {
    pub gateway: SgGateway,
    pub routes: BTreeMap<String, SgHttpRoute>,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
pub struct Config {
    pub gateways: BTreeMap<String, ConfigItem>,
    #[cfg_attr(feature = "typegen", ts(type = "Record<string, PluginConfig>"))]
    pub plugins: PluginInstanceMap,
}
