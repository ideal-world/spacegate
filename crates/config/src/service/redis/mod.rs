use crate::{service::config_format::ConfigFormat, BoxResult};
use deadpool_redis::{Connection, Pool};
pub mod create;

/// `hash: {gateway name} -> {gateway config}`
pub const CONF_GATEWAY_KEY: &str = "sg:conf:gateway";
/// `hash: {gateway name} -> {<http route name> -> <http route config>}`
pub const CONF_HTTP_ROUTE_KEY: &str = "sg:conf:route:http:";
/// `hash: {plugin instance id} -> {config}`
pub const CONF_PLUGIN_KEY: &str = "sg:conf:plugin";
/// string: {timestamp}##{changed obj}##{method}##{changed gateway name}##{changed route name} -> None
/// changed obj: gateway/httproute
/// method: create/update/delete
/// changed route name: None or <route name>
pub const CONF_CHANGE_TRIGGER: &str = "sg:conf:change:trigger:";

pub struct Redis<F> {
    redis_conn_pool: Pool,
    pub format: F,
}

impl<F> Redis<F>
where
    F: ConfigFormat,
{
    pub fn new(redis_conn_pool: Pool, format: F) -> Self {
        Self { redis_conn_pool, format }
    }

    pub async fn get_con(&self) -> BoxResult<Connection> {
        Ok(self.redis_conn_pool.get().await.map_err(Box::new)?)
    }
}
