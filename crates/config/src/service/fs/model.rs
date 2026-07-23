use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hasher},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use spacegate_model::{constants::DEFAULT_API_PORT, ConfigItem, ObservabilityConfig, PluginBinding, PluginInstanceId, PluginInstanceMap, PluginInstanceName, SgGateway, SgRoute};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum FsAsmPluginConfig {
    Anon {
        uid: String,
        code: String,
        spec: Value,
        #[serde(default)]
        priority: i32,
    },
    Named {
        name: String,
        code: String,
        #[serde(default)]
        priority: i32,
    },
    Mono {
        code: String,
        #[serde(default)]
        priority: i32,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub(crate) enum FsAsmPluginConfigMaybeUninitialized {
    Anon {
        uid: Option<String>,
        code: String,
        spec: Value,
        #[serde(default)]
        priority: i32,
    },
    Named {
        name: String,
        code: String,
        #[serde(default)]
        priority: i32,
    },
    Mono {
        code: String,
        #[serde(default)]
        priority: i32,
    },
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
    // for config usage, list is preferred over map
    pub gateways: Vec<MainFileConfigItem<P>>,
    pub plugins: PluginConfigs,
    pub api_port: u16,
    pub observability: ObservabilityConfig,
}

impl<P> Default for MainFileConfig<P> {
    fn default() -> Self {
        MainFileConfig {
            gateways: Default::default(),
            plugins: Default::default(),
            api_port: DEFAULT_API_PORT,
            observability: Default::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct MainFileConfigItem<P = FsAsmPluginConfig> {
    #[serde(flatten)]
    pub gateway: SgGateway<P>,
    pub routes: Vec<SgRoute<P>>,
}
impl<P> Default for MainFileConfigItem<P> {
    fn default() -> Self {
        MainFileConfigItem {
            gateway: Default::default(),
            routes: Default::default(),
        }
    }
}
impl<P> From<ConfigItem<P>> for MainFileConfigItem<P> {
    fn from(value: ConfigItem<P>) -> Self {
        MainFileConfigItem {
            gateway: value.gateway,
            routes: value.routes.into_values().collect(),
        }
    }
}

impl<P> From<MainFileConfigItem<P>> for ConfigItem<P> {
    fn from(val: MainFileConfigItem<P>) -> Self {
        ConfigItem {
            gateway: val.gateway,
            routes: val.routes.into_iter().map(|route| (route.route_name().to_string(), route)).collect(),
        }
    }
}

impl<P> MainFileConfigItem<P> {
    pub fn map_plugins<F, T>(self, mut f: F) -> MainFileConfigItem<T>
    where
        F: FnMut(P) -> T,
    {
        MainFileConfigItem {
            gateway: self.gateway.map_plugins(&mut f),
            routes: self.routes.into_iter().map(|route| route.map_plugins(&mut f)).collect(),
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
            FsAsmPluginConfigMaybeUninitialized::Anon { uid, code, spec, priority } => {
                let uid = if let Some(uid) = uid {
                    hasher.write(uid.as_bytes());
                    uid
                } else {
                    hasher.write(code.as_bytes());
                    hasher.write(spec.to_string().as_bytes());
                    format!("{:016x}", hasher.finish())
                };
                FsAsmPluginConfig::Anon { uid, code, spec, priority }
            }
            FsAsmPluginConfigMaybeUninitialized::Named { name, code, priority } => FsAsmPluginConfig::Named { name, code, priority },
            FsAsmPluginConfigMaybeUninitialized::Mono { code, priority } => FsAsmPluginConfig::Mono { code, priority },
        };
        let gateways = self.gateways.into_iter().map(|item| item.map_plugins(&mut set_uid)).collect();
        MainFileConfig {
            gateways,
            plugins: self.plugins,
            api_port: self.api_port,
            observability: self.observability,
        }
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
            FsAsmPluginConfig::Anon { uid, code, spec, priority } => {
                let id = PluginInstanceId {
                    code: code.into(),
                    name: spacegate_model::PluginInstanceName::Anon { uid },
                };
                plugins.insert(id.clone(), spec);
                PluginBinding::from(id).with_priority(priority)
            }
            FsAsmPluginConfig::Named { name, code, priority } => PluginBinding::new(code, PluginInstanceName::Named { name }, priority),
            FsAsmPluginConfig::Mono { code, priority } => PluginBinding::new(code, PluginInstanceName::Mono {}, priority),
        };
        let gateways = self
            .gateways
            .into_iter()
            .map(|item| {
                let config_item: ConfigItem = item.map_plugins(&mut collect_plugin).into();
                (config_item.gateway.name.clone(), config_item)
            })
            .collect();
        spacegate_model::Config {
            gateways,
            plugins,
            api_port: Some(self.api_port),
            observability: self.observability,
        }
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
        let mut map = |binding: PluginBinding| match binding.id.name {
            PluginInstanceName::Anon { uid } => {
                if let Some(spec) = anon_plugins.remove(&uid) {
                    FsAsmPluginConfig::Anon {
                        uid,
                        code: binding.id.code.into(),
                        spec,
                        priority: binding.priority,
                    }
                } else {
                    FsAsmPluginConfig::Anon {
                        uid,
                        code: binding.id.code.into(),
                        spec: Default::default(),
                        priority: binding.priority,
                    }
                }
            }
            PluginInstanceName::Named { name } => FsAsmPluginConfig::Named {
                name,
                code: binding.id.code.into(),
                priority: binding.priority,
            },
            PluginInstanceName::Mono {} => FsAsmPluginConfig::Mono {
                code: binding.id.code.into(),
                priority: binding.priority,
            },
        };
        let gateways = value.gateways.into_values().map(|item| <MainFileConfigItem<FsAsmPluginConfig>>::from(item.map_plugins(&mut map))).collect();
        MainFileConfig {
            gateways,
            plugins,
            api_port: value.api_port.unwrap_or(DEFAULT_API_PORT),
            observability: value.observability,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::MainFileConfig;

    #[test]
    fn preserves_plugin_binding_priority_through_file_config_round_trip() {
        let config: MainFileConfig = serde_json::from_value(json!({
            "gateways": [{
                "name": "test-gateway",
                "plugins": [
                    { "code": "native", "kind": "named", "name": "legacy" },
                    { "code": "native", "kind": "named", "name": "negative", "priority": -20 },
                    { "code": "wasm", "kind": "named", "name": "first", "priority": 100 },
                    { "code": "wasm", "kind": "named", "name": "second", "priority": 100 }
                ]
            }]
        }))
        .expect("file config should deserialize");

        let output = MainFileConfig::from(config.into_model_config());
        let value = serde_json::to_value(output).expect("file config should serialize");

        let plugins = value["gateways"][0]["plugins"].as_array().unwrap();
        assert_eq!(plugins[0]["priority"], 0);
        assert_eq!(plugins[1]["priority"], -20);
        assert_eq!(plugins[2]["priority"], 100);
        assert_eq!(plugins[3]["priority"], 100);
    }
}
