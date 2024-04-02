use std::{borrow::Cow, collections::HashMap};

use serde::{Deserialize, Serialize};
use spacegate_ext_axum::{
    axum::{self, extract::Query, Router},
    GlobalAxumServer,
};

use crate::{
    instance::{PluginInstanceId, PluginInstanceSnapshot},
    PluginRepoSnapshot,
};

impl PluginInstanceId {
    pub async fn route(&self, router: spacegate_ext_axum::axum::Router) {
        let path = format!("/plugin/{code}/instance/{name}", code = self.code, name = self.name);
        spacegate_ext_axum::GlobalAxumServer::default().modify_router(move |r| r.nest(&path, router)).await;
    }
}

pub async fn register_plugin_routes() {
    let server = GlobalAxumServer::default();
    server
        .modify_router(|mut router| {
            router = router.nest(
                "/plugin-snapshot",
                Router::new().route("/repo", axum::routing::get(repo_snapshot)).route("/instance", axum::routing::get(instance_snapshot)),
            );
            #[cfg(feature = "schema")]
            {
                router = router.route("/plugin-schema", axum::routing::get(plugin_schema));
            }
            router
        })
        .await
}

pub async fn repo_snapshot() -> axum::Json<HashMap<Cow<'static, str>, PluginRepoSnapshot>> {
    axum::Json(crate::SgPluginRepository::global().repo_snapshot())
}

pub async fn instance_snapshot(Query(instance_id): Query<PluginInstanceId>) -> axum::Json<Option<PluginInstanceSnapshot>> {
    axum::Json(crate::SgPluginRepository::global().instance_snapshot(instance_id))
}

#[cfg(feature = "schema")]
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginCode {
    code: String,
}

#[cfg(feature = "schema")]
pub async fn plugin_schema(Query(PluginCode { code }): Query<PluginCode>) -> axum::Json<Option<schemars::schema::RootSchema>> {
    let code: Cow<'static, str> = code.into();
    let schema = crate::SgPluginRepository::global().plugins.read().expect("poisoned").get(&code).and_then(|p| p.schema.clone());
    axum::Json(schema)
}
