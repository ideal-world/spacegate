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

const PLUGIN_CONFIG_FORMAT: &str = "plugin_config_v1";

/// 插件配置的版本化持久化信封，用于将管理元数据与运行时 spec 分开保存。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPluginConfig {
    #[serde(rename = "_spacegate_format")]
    format: String,
    #[serde(flatten)]
    config: PluginConfig,
}

/// 规范化可选展示名称，空白名称按未配置处理。
pub fn normalize_plugin_display_name(display_name: Option<String>) -> Option<String> {
    display_name.and_then(|name| {
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

/// 将插件配置编码为带版本标识的持久化值。
pub(crate) fn encode_stored_plugin_config(mut config: PluginConfig) -> Result<Value, BoxError> {
    config.display_name = normalize_plugin_display_name(config.display_name);
    Ok(serde_json::to_value(StoredPluginConfig {
        format: PLUGIN_CONFIG_FORMAT.to_string(),
        config,
    })?)
}

/// 解码插件持久化值；未带版本标识时按旧版裸 spec 读取。
pub(crate) fn decode_stored_plugin_config(id: &PluginInstanceId, value: Value, accept_legacy_full_config: bool) -> Result<PluginConfig, BoxError> {
    if value.get("_spacegate_format").and_then(Value::as_str) == Some(PLUGIN_CONFIG_FORMAT) {
        let mut stored: StoredPluginConfig = serde_json::from_value(value)?;
        stored.config.id = id.clone();
        stored.config.display_name = normalize_plugin_display_name(stored.config.display_name);
        return Ok(stored.config);
    }

    if accept_legacy_full_config {
        if let Ok(mut config) = serde_json::from_value::<PluginConfig>(value.clone()) {
            config.id = id.clone();
            config.display_name = normalize_plugin_display_name(config.display_name);
            return Ok(config);
        }
    }

    Ok(PluginConfig {
        id: id.clone(),
        display_name: None,
        spec: value,
    })
}

pub trait Create: Sync + Send {
    fn create_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgRoute) -> impl Future<Output = Result<(), BoxError>> + Send;
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
    fn create_plugin(&self, config: PluginConfig) -> impl Future<Output = Result<(), BoxError>> + Send;
}

pub trait Update: Sync + Send {
    fn update_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgRoute) -> impl Future<Output = Result<(), BoxError>> + Send;

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
    fn update_plugin(&self, config: PluginConfig) -> impl Future<Output = Result<(), BoxError>> + Send;
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

/// Coordinates a route identity change while keeping the complete replacement route intact.
pub trait Rename: Create + Delete + Retrieve {
    /// Creates the new route identity before removing the old identity.
    fn rename_config_item_route(&self, gateway_name: &str, old_route_name: &str, new_route_name: &str, route: SgRoute) -> impl Future<Output = Result<(), BoxError>> + Send {
        async move {
            if old_route_name == new_route_name {
                return Err("route rename requires different names".into());
            }
            if route.route_name() != new_route_name {
                return Err("route payload name does not match rename target".into());
            }
            if self.retrieve_config_item_route(gateway_name, old_route_name).await?.is_none() {
                return Err(format!("route [{old_route_name}] not found").into());
            }
            if self.retrieve_config_item_route(gateway_name, new_route_name).await?.is_some() {
                return Err(format!("route [{new_route_name}] already exists").into());
            }

            self.create_config_item_route(gateway_name, new_route_name, route).await?;
            self.delete_config_item_route(gateway_name, old_route_name).await
        }
    }
}

impl<T> Rename for T where T: Create + Delete + Retrieve {}

pub trait Retrieve: Sync + Send {
    fn retrieve_config_item_gateway(&self, gateway_name: &str) -> impl Future<Output = Result<Option<SgGateway>, BoxError>> + Send;
    fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> impl Future<Output = Result<Option<SgRoute>, BoxError>> + Send;
    fn retrieve_config_item_route_names(&self, name: &str) -> impl Future<Output = Result<Vec<String>, BoxError>> + Send;
    fn retrieve_config_item_all_routes(&self, name: &str) -> impl Future<Output = Result<BTreeMap<String, SgRoute>, BoxError>> + Send {
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
                observability: Default::default(),
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RenameTestBackend {
        routes: Mutex<BTreeMap<String, SgRoute>>,
        operations: Mutex<Vec<&'static str>>,
    }

    impl Create for RenameTestBackend {
        async fn create_config_item_gateway(&self, _gateway_name: &str, _gateway: SgGateway) -> Result<(), BoxError> {
            Ok(())
        }

        async fn create_config_item_route(&self, _gateway_name: &str, route_name: &str, route: SgRoute) -> Result<(), BoxError> {
            self.routes.lock().expect("routes lock").insert(route_name.to_string(), route);
            self.operations.lock().expect("operations lock").push("create");
            Ok(())
        }

        async fn create_plugin(&self, _config: PluginConfig) -> Result<(), BoxError> {
            Ok(())
        }
    }

    impl Delete for RenameTestBackend {
        async fn delete_config_item_gateway(&self, _gateway_name: &str) -> Result<(), BoxError> {
            Ok(())
        }

        async fn delete_config_item_route(&self, _gateway_name: &str, route_name: &str) -> Result<(), BoxError> {
            self.routes.lock().expect("routes lock").remove(route_name);
            self.operations.lock().expect("operations lock").push("delete");
            Ok(())
        }

        async fn delete_plugin(&self, _id: &PluginInstanceId) -> Result<(), BoxError> {
            Ok(())
        }
    }

    impl Retrieve for RenameTestBackend {
        async fn retrieve_config_item_gateway(&self, _gateway_name: &str) -> Result<Option<SgGateway>, BoxError> {
            Ok(None)
        }

        async fn retrieve_config_item_route(&self, _gateway_name: &str, route_name: &str) -> Result<Option<SgRoute>, BoxError> {
            Ok(self.routes.lock().expect("routes lock").get(route_name).cloned())
        }

        async fn retrieve_config_item_route_names(&self, _name: &str) -> Result<Vec<String>, BoxError> {
            Ok(self.routes.lock().expect("routes lock").keys().cloned().collect())
        }

        async fn retrieve_config_names(&self) -> Result<Vec<String>, BoxError> {
            Ok(vec![])
        }

        async fn retrieve_all_plugins(&self) -> Result<Vec<PluginConfig>, BoxError> {
            Ok(vec![])
        }

        async fn retrieve_plugin(&self, _id: &PluginInstanceId) -> Result<Option<PluginConfig>, BoxError> {
            Ok(None)
        }

        async fn retrieve_plugins_by_code(&self, _code: &str) -> Result<Vec<PluginConfig>, BoxError> {
            Ok(vec![])
        }
    }

    fn route(name: &str) -> SgRoute {
        let mut route = SgHttpRoute::default();
        route.route_name = name.to_string();
        route.into()
    }

    #[test]
    fn rename_creates_the_complete_new_route_before_removing_the_old_route() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(async {
            let backend = RenameTestBackend::default();
            backend.routes.lock().expect("routes lock").insert("orders-v1".to_string(), route("orders-v1"));
            let replacement = route("orders-v2");

            backend.rename_config_item_route("edge", "orders-v1", "orders-v2", replacement.clone()).await.unwrap();

            let routes = backend.routes.lock().expect("routes lock");
            assert!(routes.get("orders-v1").is_none());
            assert_eq!(serde_json::to_value(routes.get("orders-v2")).unwrap(), serde_json::to_value(Some(replacement)).unwrap());
            assert_eq!(*backend.operations.lock().expect("operations lock"), ["create", "delete"]);
        });
    }

    #[test]
    fn rename_rejects_an_existing_target_without_touching_the_old_route() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(async {
            let backend = RenameTestBackend::default();
            backend.routes.lock().expect("routes lock").extend([("orders-v1".to_string(), route("orders-v1")), ("orders-v2".to_string(), route("orders-v2"))]);

            assert!(backend.rename_config_item_route("edge", "orders-v1", "orders-v2", route("orders-v2")).await.is_err());

            assert_eq!(backend.routes.lock().expect("routes lock").len(), 2);
            assert!(backend.operations.lock().expect("operations lock").is_empty());
        });
    }

    #[test]
    fn rename_rejects_a_payload_name_that_does_not_match_the_target() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(async {
            let backend = RenameTestBackend::default();
            backend.routes.lock().expect("routes lock").insert("orders-v1".to_string(), route("orders-v1"));

            assert!(backend.rename_config_item_route("edge", "orders-v1", "orders-v2", route("orders-v3")).await.is_err());
            assert_eq!(backend.routes.lock().expect("routes lock").len(), 1);
            assert!(backend.operations.lock().expect("operations lock").is_empty());
        });
    }
}
