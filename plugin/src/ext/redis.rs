pub mod plugins;

use crate::instance::PluginInstanceId;

impl PluginInstanceId {
    #[cfg(feature = "redis")]
    pub fn redis_prefix(&self) -> String {
        let id = self.name.to_string();
        let code = self.code.as_ref();
        format!("sg:plugin:{code}:{id}")
    }
}
