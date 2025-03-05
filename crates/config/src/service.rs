/// Config file format
pub mod config_format;
/// File system backend
#[cfg(feature = "fs")]
pub mod fs;
/// Kubernetes backend
#[cfg(feature = "k8s")]
pub mod k8s;
/// In-memory backend
pub mod memory;
/// Redis backend
#[cfg(feature = "redis")]
pub mod redis;
use std::{collections::BTreeMap, error::Error, fmt::Display, str::FromStr};

use futures_util::Future;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use spacegate_model::*;

pub trait Create: Sync + Send {
    fn create_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgHttpRoute) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn create_config_item(&self, name: &str, item: ConfigItem) -> impl Future<Output = Result<(), BoxError>> + Send {
        async move {
            self.create_config_item_gateway(name, item.gateway).await?;
            for (route_name, route) in item.routes {
                self.create_config_item_route(name, &route_name, route).await?;
            }
            Ok(())
        }
    }
    fn create_config(&self, config: Config) -> impl Future<Output = Result<(), BoxError>> + Send {
        async move {
            for (name, item) in config.gateways {
                self.create_config_item(&name, item).await?;
            }
            Ok(())
        }
    }
    fn create_plugin(&self, id: &PluginInstanceId, value: Value) -> impl Future<Output = Result<(), BoxError>> + Send;
}

pub trait Update: Sync + Send {
    fn update_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgHttpRoute) -> impl Future<Output = Result<(), BoxError>> + Send;

    fn update_config_item(&self, name: &str, item: ConfigItem) -> impl Future<Output = Result<(), BoxError>> + Send {
        async move {
            self.update_config_item_gateway(name, item.gateway).await?;
            for (route_name, route) in item.routes {
                self.update_config_item_route(name, &route_name, route).await?;
            }
            Ok(())
        }
    }
    fn update_config(&self, config: Config) -> impl Future<Output = Result<(), BoxError>> + Send {
        async move {
            for (name, item) in config.gateways {
                self.update_config_item(&name, item).await?;
            }
            Ok(())
        }
    }
    fn update_plugin(&self, id: &PluginInstanceId, value: Value) -> impl Future<Output = Result<(), BoxError>> + Send;
}

pub trait Delete: Sync + Send {
    fn delete_config_item_gateway(&self, gateway_name: &str) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn delete_config_item_all_routes(&self, gateway_name: &str) -> impl Future<Output = Result<(), BoxError>> + Send
    where
        Self: Retrieve,
    {
        async move {
            for route_name in self.retrieve_config_item_route_names(gateway_name).await? {
                self.delete_config_item_route(gateway_name, &route_name).await?;
            }
            Ok(())
        }
    }
    fn delete_config_item(&self, name: &str) -> impl Future<Output = Result<(), BoxError>> + Send
    where
        Self: Retrieve,
    {
        async move {
            self.delete_config_item_gateway(name).await?;
            self.delete_config_item_all_routes(name).await?;
            Ok(())
        }
    }
    fn delete_plugin(&self, id: &PluginInstanceId) -> impl Future<Output = Result<(), BoxError>> + Send;
}

pub trait Retrieve: Sync + Send {
    fn retrieve_config_item_gateway(&self, gateway_name: &str) -> impl Future<Output = Result<Option<SgGateway>, BoxError>> + Send;
    fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> impl Future<Output = Result<Option<SgHttpRoute>, BoxError>> + Send;
    fn retrieve_config_item_route_names(&self, name: &str) -> impl Future<Output = Result<Vec<String>, BoxError>> + Send;
    fn retrieve_config_item_all_routes(&self, name: &str) -> impl Future<Output = Result<BTreeMap<String, SgHttpRoute>, BoxError>> + Send {
        async move {
            let mut routes = BTreeMap::new();
            for route_name in self.retrieve_config_item_route_names(name).await? {
                if let Ok(Some(route)) = self.retrieve_config_item_route(name, &route_name).await {
                    routes.insert(route_name, route);
                }
            }
            Ok(routes)
        }
    }
    fn retrieve_config_item(&self, name: &str) -> impl Future<Output = Result<Option<ConfigItem>, BoxError>> + Send {
        async move {
            let Some(gateway) = self.retrieve_config_item_gateway(name).await? else {
                return Ok(None);
            };
            let routes = self.retrieve_config_item_all_routes(name).await?;
            Ok(Some(ConfigItem { gateway, routes }))
        }
    }
    fn retrieve_config_names(&self) -> impl Future<Output = Result<Vec<String>, BoxError>> + Send;
    fn retrieve_config(&self) -> impl Future<Output = Result<Config, BoxError>> + Send
    where
        Self: Sync,
        BoxError: Send,
    {
        async move {
            let mut gateways = BTreeMap::new();
            for name in self.retrieve_config_names().await? {
                if let Some(item) = self.retrieve_config_item(&name).await? {
                    gateways.insert(name, item);
                }
            }
            let plugins = self.retrieve_all_plugins().await?;
            Ok(Config {
                gateways,
                plugins: PluginInstanceMap::from_config_vec(plugins),
                api_port: None,
            })
        }
    }
    fn retrieve_all_plugins(&self) -> impl Future<Output = Result<Vec<PluginConfig>, BoxError>> + Send;
    fn retrieve_plugin(&self, id: &PluginInstanceId) -> impl Future<Output = Result<Option<PluginConfig>, BoxError>> + Send;
    fn retrieve_plugins_by_code(&self, code: &str) -> impl Future<Output = Result<Vec<PluginConfig>, BoxError>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigEventType {
    Create,
    Update,
    Delete,
}

impl FromStr for ConfigEventType {
    type Err = BoxError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "create" => Ok(Self::Create),
            "update" => Ok(Self::Update),
            "delete" => Ok(Self::Delete),
            _ => Err(format!("unknown ConfigEventType: {}", s).into()),
        }
    }
}

impl Display for ConfigEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create => write!(f, "create"),
            Self::Update => write!(f, "update"),
            Self::Delete => write!(f, "delete"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ConfigType {
    Gateway {
        name: String,
    },
    Route {
        gateway_name: String,
        name: String,
    },
    Plugin {
        id: PluginInstanceId,
    },
    /// update global config, the shell would reload all
    Global,
}

impl Display for ConfigType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gateway { name } => write!(f, "gateway/{}", name),
            Self::Route { gateway_name, name } => write!(f, "httproute/{}/{}", gateway_name, name),
            Self::Plugin { id } => write!(f, "plugin/{}/{}", id.code, id.name),
            Self::Global => write!(f, "global"),
        }
    }
}

impl FromStr for ConfigType {
    type Err = BoxError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let f = s.split('/').collect::<Vec<_>>();
        match &f[..] {
            ["gateway", gateway_name] => Ok(Self::Gateway { name: gateway_name.to_string() }),
            ["httproute", gateway, route_name] => Ok(Self::Route {
                gateway_name: gateway.to_string(),
                name: route_name.to_string(),
            }),
            ["plugin", code, name] => {
                let name = PluginInstanceName::from_str(name)?;
                Ok(Self::Plugin {
                    id: PluginInstanceId::new(code.to_string(), name),
                })
            }
            _ => Err(format!("unknown ConfigType: {}", s).into()),
        }
    }
}

pub trait CreateListener {
    const CONFIG_LISTENER_NAME: &'static str;
    type Listener: Listen;
    fn create_listener(&self) -> impl Future<Output = Result<(Config, Self::Listener), Box<dyn Error + Sync + Send + 'static>>> + Send;
}
pub trait Instance: Send + Sync {
    fn id(&self) -> &str;
    fn api_url(&self) -> &str;
}
pub trait Discovery: 'static {
    fn instances(&self) -> impl Future<Output = Result<Vec<impl Instance>, BoxError>> + Send;
    fn backends(&self) -> impl Future<Output = Result<Vec<BackendHost>, BoxError>> + Send {
        std::future::ready(Ok(vec![]))
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ListenEvent {
    pub r#type: ConfigEventType,
    pub config: ConfigType,
}

impl From<(ConfigType, ConfigEventType)> for ListenEvent {
    fn from((config, r#type): (ConfigType, ConfigEventType)) -> Self {
        Self { r#type, config }
    }
}

pub trait Listen: Unpin {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<ListenEvent, BoxError>>;
}

pub trait ListenExt: Listen {
    fn join<L1>(self, l1: L1) -> Joint<Self, L1>
    where
        L1: Listen,
        Self: Sized,
    {
        Joint { l0: self, l1 }
    }
}

impl<T: Listen> ListenExt for T {}

pub struct Joint<L0, L1> {
    l0: L0,
    l1: L1,
}

impl<L0, L1> Listen for Joint<L0, L1>
where
    L0: Listen,
    L1: Listen,
{
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<ListenEvent, BoxError>> {
        // l0 has higher priority
        let l0 = self.l0.poll_next(cx);
        if l0.is_ready() {
            return l0;
        }
        self.l1.poll_next(cx)
    }
}

impl Listen for tokio::sync::mpsc::Receiver<ListenEvent> {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<ListenEvent, BoxError>> {
        self.poll_recv(cx).map(|r| r.ok_or("channel closed".into()))
    }
}
