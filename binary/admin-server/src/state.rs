use std::sync::Arc;

use crate::mw;
use spacegate_config::{
    service::{ConfigEventType, ConfigType, ListenEvent},
    PluginInstanceId,
};
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginCode(String);

impl PluginCode {
    pub fn plugin(plugin_name: impl AsRef<str>) -> Self {
        Self(plugin_name.as_ref().to_string())
    }
}

impl std::fmt::Display for PluginCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 标识插件配置变更由哪个控制面通道同步到运行时。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginRuntimeSync {
    /// 文件配置没有细粒度监听事件，admin-server 需要主动通知运行时。
    File,
    /// Kubernetes 由资源 watch 生成运行时事件，避免 admin-server 重复推送。
    Kubernetes,
}

impl PluginRuntimeSync {
    /// 为需要主动同步的配置后端创建插件运行时事件。
    pub fn plugin_event(self, id: PluginInstanceId, event_type: ConfigEventType) -> Option<ListenEvent> {
        match self {
            Self::File => Some((ConfigType::Plugin { id }, event_type).into()),
            Self::Kubernetes => None,
        }
    }
}

pub struct AppState<B> {
    pub backend: Arc<B>,
    pub plugin_runtime_sync: PluginRuntimeSync,
    pub version: mw::version_control::Version,
    pub secret: Option<Arc<[u8]>>,
    pub sk_digest: Option<Arc<[u8; 32]>>,
    // pub plugin_schemas: Arc<RwLock<HashMap<PluginCode, serde_json::Value>>>,
}

impl<B> Clone for AppState<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            plugin_runtime_sync: self.plugin_runtime_sync,
            version: self.version.clone(),
            secret: self.secret.clone(),
            sk_digest: self.sk_digest.clone(),
            // plugin_schemas: self.plugin_schemas.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use spacegate_config::{
        service::{ConfigEventType, ConfigType},
        PluginInstanceId, PluginInstanceName,
    };

    use super::PluginRuntimeSync;

    #[test]
    fn file_mode_creates_runtime_plugin_event() {
        let id = PluginInstanceId::new("wasm", PluginInstanceName::named("route-binding"));

        let event = PluginRuntimeSync::File.plugin_event(id.clone(), ConfigEventType::Create);

        assert!(matches!(
            event,
            Some(event) if matches!(&event.config, ConfigType::Plugin { id: event_id } if event_id == &id)
                && matches!(event.r#type, ConfigEventType::Create)
        ));
    }

    #[test]
    fn kubernetes_mode_defers_plugin_updates_to_its_watch() {
        let id = PluginInstanceId::new("wasm", PluginInstanceName::named("route-binding"));

        assert!(PluginRuntimeSync::Kubernetes.plugin_event(id, ConfigEventType::Update).is_none());
    }
}
