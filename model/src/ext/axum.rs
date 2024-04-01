use crate::PluginInstanceId;
impl PluginInstanceId {
    pub async fn route(&self, router: spacegate_ext_axum::axum::Router) {
        let code = self.code.as_ref();
        let name = match &self.name {
            crate::PluginInstanceName::Anon { uid } => uid.to_string(),
            crate::PluginInstanceName::Named { name } => name.clone(),
            crate::PluginInstanceName::Mono {} => "*".to_string(),
        };
        let path = format!("/plugin/{code}/instance/{name}");
        spacegate_ext_axum::GlobalAxumServer::default().modify_router(move |r| r.nest(&path, router)).await;
    }
}
