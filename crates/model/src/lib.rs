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
#[serde(default)]
pub struct ConfigItem<P = PluginInstanceId> {
    pub gateway: SgGateway<P>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
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

impl<P> Default for ConfigItem<P> {
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct Config {
    pub gateways: BTreeMap<String, ConfigItem<PluginInstanceId>>,
    #[cfg_attr(feature = "typegen", ts(as = "crate::plugin::PluginInstanceMapTs"))]
    pub plugins: PluginInstanceMap,
    pub api_port: Option<u16>,
}

#[allow(clippy::derivable_impls)]
impl Default for Config {
    fn default() -> Self {
        Self {
            gateways: Default::default(),
            plugins: Default::default(),
            #[cfg(feature = "axum")]
            api_port: Some(crate::constants::DEFAULT_API_PORT),
            #[cfg(not(feature = "axum"))]
            api_port: None,
        }
    }
}
