use std::{borrow::Cow, collections::HashMap};

use serde::{Deserialize, Serialize};
use spacegate_ext_axum::{
    axum::{self, extract::Query, Router},
    GlobalAxumServer,
};
use spacegate_model::{PluginAttributes, PluginInstanceId};

use crate::{instance::PluginInstanceSnapshot, PluginRepoSnapshot};

pub async fn register_plugin_routes() {
    let server = GlobalAxumServer::default();
    server
        .modify_router(|mut router| {
            router = router
                .nest(
                    "/plugin-snapshot",
                    Router::new().route("/repo", axum::routing::get(repo_snapshot)).route("/instance", axum::routing::get(instance_snapshot)),
                )
                .route("/plugin-list", axum::routing::get(plugin_list));
            #[cfg(feature = "schema")]
            {
                router = router.route("/plugin-schema", axum::routing::get(plugin_schema));
            }
            router
        })
        .await
}

pub async fn repo_snapshot() -> axum::Json<HashMap<String, PluginRepoSnapshot>> {
    axum::Json(crate::SgPluginRepository::global().repo_snapshot())
}

pub async fn instance_snapshot(Query(instance_id): Query<PluginInstanceId>) -> axum::Json<Option<PluginInstanceSnapshot>> {
    axum::Json(crate::SgPluginRepository::global().instance_snapshot(instance_id))
}

pub async fn plugin_list() -> axum::Json<Vec<PluginAttributes>> {
    axum::Json(crate::SgPluginRepository::global().plugin_list())
}

#[cfg(feature = "schema")]
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginCode {
    code: String,
}

#[cfg(feature = "schema")]
pub async fn plugin_schema(Query(PluginCode { code }): Query<PluginCode>) -> axum::Json<Option<schemars::schema::RootSchema>> {
    let schema = crate::SgPluginRepository::global().plugins.read().expect("poisoned").get(&code).and_then(|p| p.schema.clone());
    axum::Json(schema)
}
