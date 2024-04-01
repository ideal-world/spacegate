pub mod plugin;
use std::{collections::BTreeMap, fmt::Debug};

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

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub struct ConfigItem<P = PluginInstanceId> {
    pub gateway: SgGateway<P>,
    pub routes: BTreeMap<String, SgHttpRoute<P>>,
}

impl<P> ConfigItem<P> {
    pub fn map_plugins<F, T>(self, mut f: F) -> ConfigItem<T>
    where
        F: FnMut(P) -> T,
    {
        ConfigItem {
            gateway: self.gateway.map_plugins(&mut f),
            routes: self.routes.into_iter().map(|(k, r)| (k, r.map_plugins(&mut f))).collect(),
        }
    }
}

impl<P: Default> Default for ConfigItem<P> {
    fn default() -> Self {
        Self {
            gateway: Default::default(),
            routes: Default::default(),
        }
    }
}

impl<P: Debug> std::fmt::Debug for ConfigItem<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigItem").field("gateway", &self.gateway).field("routes", &self.routes).finish()
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct Config {
    pub gateways: BTreeMap<String, ConfigItem<PluginInstanceId>>,
    #[cfg_attr(feature = "typegen", ts(as = "crate::plugin::PluginInstanceMapTs"))]
    pub plugins: PluginInstanceMap,
}
