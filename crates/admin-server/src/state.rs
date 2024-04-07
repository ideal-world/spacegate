use std::{collections::HashMap, sync::Arc};

use axum::http::Uri;

use crate::mw;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginCode(String);

impl PluginCode {
    pub fn plugin(plugin_name: impl AsRef<str>) -> Self {
        Self(plugin_name.as_ref().to_string())
    }
}

impl ToString for PluginCode {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

#[derive(Debug)]
pub struct AppState<B> {
    pub backend: Arc<B>,
    pub version: mw::version_control::Version,
    pub plugin_schemas: Arc<HashMap<PluginCode, serde_json::Value>>,
}

impl<B> Clone for AppState<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            version: self.version.clone(),
            plugin_schemas: self.plugin_schemas.clone(),
        }
    }
}
