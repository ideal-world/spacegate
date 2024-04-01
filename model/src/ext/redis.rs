use crate::plugin::PluginInstanceId;

impl PluginInstanceId {
    pub fn redis_prefix(&self) -> String {
        let code = self.code.as_ref();
        match &self.name {
            crate::PluginInstanceName::Anon { uid } => format!("sg:plugin:{code}:{uid}"),
            crate::PluginInstanceName::Named { name } => format!("sg:plugin:{code}:{name}"),
            crate::PluginInstanceName::Mono {} => format!("sg:plugin:{code}"),
        }
    }
}
