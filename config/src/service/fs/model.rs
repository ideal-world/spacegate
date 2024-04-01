use std::{
    collections::{BTreeMap, HashMap},
    hash::{DefaultHasher, Hasher},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use spacegate_model::{ConfigItem, PluginInstanceId, PluginInstanceMap, PluginInstanceName};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum FsAsmPluginConfig {
    Anon { uid: u64, code: String, spec: Value },
    Named { name: String, code: String },
    Mono { code: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub(crate) enum FsAsmPluginConfigMaybeUninitialized {
    Anon { uid: Option<u64>, code: String, spec: Value },
    Named { name: String, code: String },
    Mono { code: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct FsNamedPluginConfig {
    pub name: String,
    pub code: String,
    pub spec: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct FsMonoPluginConfig {
    pub code: String,
    pub spec: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct FsAnonPluginConfig {
    pub code: String,
    pub spec: Value,
    pub uid: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct MainFileConfig<P = FsAsmPluginConfig> {
    pub gateways: BTreeMap<String, ConfigItem<P>>,
    pub plugins: PluginConfigs,
}

impl<P> Default for MainFileConfig<P> {
    fn default() -> Self {
        MainFileConfig {
            gateways: Default::default(),
            plugins: Default::default(),
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PluginConfigs {
    pub named: Vec<FsNamedPluginConfig>,
    pub mono: Vec<FsMonoPluginConfig>,
}

impl MainFileConfig<FsAsmPluginConfigMaybeUninitialized> {
    pub fn initialize_uid(self) -> MainFileConfig<FsAsmPluginConfig> {
        let mut hasher = DefaultHasher::new();
        let mut set_uid = move |p: FsAsmPluginConfigMaybeUninitialized| match p {
            FsAsmPluginConfigMaybeUninitialized::Anon { uid, code, spec } => {
                let uid = if let Some(uid) = uid {
                    hasher.write(&uid.to_be_bytes());
                    uid
                } else {
                    hasher.write(code.as_bytes());
                    hasher.write(spec.to_string().as_bytes());
                    hasher.finish()
                };
                FsAsmPluginConfig::Anon { uid, code, spec }
            }
            FsAsmPluginConfigMaybeUninitialized::Named { name, code } => FsAsmPluginConfig::Named { name, code },
            FsAsmPluginConfigMaybeUninitialized::Mono { code } => FsAsmPluginConfig::Mono { code },
        };
        let gateways = self.gateways.into_iter().map(|(name, item)| (name, item.map_plugins(&mut set_uid))).collect();
        MainFileConfig { gateways, plugins: self.plugins }
    }
}

impl MainFileConfig<FsAsmPluginConfig> {
    pub fn into_model_config(self) -> spacegate_model::Config {
        let mut plugins = PluginInstanceMap::default();
        for named in self.plugins.named {
            let id = PluginInstanceId {
                code: named.code.into(),
                name: spacegate_model::PluginInstanceName::Named { name: named.name },
            };
            plugins.insert(id.clone(), named.spec);
        }
        for mono in self.plugins.mono {
            let id = PluginInstanceId {
                code: mono.code.into(),
                name: spacegate_model::PluginInstanceName::Mono {},
            };
            plugins.insert(id.clone(), mono.spec);
        }
        let mut collect_plugin = |p: FsAsmPluginConfig| match p {
            FsAsmPluginConfig::Anon { uid, code, spec } => {
                let id = PluginInstanceId {
                    code: code.into(),
                    name: spacegate_model::PluginInstanceName::Anon { uid },
                };
                plugins.insert(id.clone(), spec);
                id
            }
            FsAsmPluginConfig::Named { name, code } => PluginInstanceId {
                code: code.into(),
                name: spacegate_model::PluginInstanceName::Named { name },
            },
            FsAsmPluginConfig::Mono { code } => PluginInstanceId {
                code: code.into(),
                name: spacegate_model::PluginInstanceName::Mono {},
            },
        };
        let gateways = self
            .gateways
            .into_iter()
            .map(|(name, item)| {
                (
                    name,
                    ConfigItem {
                        gateway: item.gateway.map_plugins(&mut collect_plugin),
                        routes: item.routes.into_iter().map(|(route_name, route)| (route_name, route.map_plugins(&mut collect_plugin))).collect(),
                    },
                )
            })
            .collect();
        spacegate_model::Config { gateways, plugins }
    }
}

impl From<spacegate_model::Config> for MainFileConfig<FsAsmPluginConfig> {
    fn from(value: spacegate_model::Config) -> Self {
        let mut plugins = PluginConfigs::default();
        let mut anon_plugins = HashMap::new();
        for (id, spec) in value.plugins.into_inner() {
            match id.name {
                PluginInstanceName::Anon { uid } => {
                    anon_plugins.insert(uid, spec);
                }
                PluginInstanceName::Named { name } => plugins.named.push(FsNamedPluginConfig { name, code: id.code.into(), spec }),
                PluginInstanceName::Mono {} => plugins.mono.push(FsMonoPluginConfig { code: id.code.into(), spec }),
            }
        }
        let mut map = |id: PluginInstanceId| match id.name {
            PluginInstanceName::Anon { uid } => {
                if let Some(spec) = anon_plugins.remove(&uid) {
                    FsAsmPluginConfig::Anon { uid, code: id.code.into(), spec }
                } else {
                    FsAsmPluginConfig::Anon {
                        uid,
                        code: id.code.into(),
                        spec: Default::default(),
                    }
                }
            }
            PluginInstanceName::Named { name } => FsAsmPluginConfig::Named { name, code: id.code.into() },
            PluginInstanceName::Mono {} => FsAsmPluginConfig::Mono { code: id.code.into() },
        };
        let gateways = value.gateways.into_iter().map(|(gw_name, item)| (gw_name, item.map_plugins(&mut map))).collect();
        MainFileConfig { gateways, plugins }
    }
}
