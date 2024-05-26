use std::str::FromStr;

use crate::{service::config_format::ConfigFormat, BoxResult};
use deadpool_redis::{Connection, Manager, Pool};
use redis::{FromRedisValue, RedisWrite, ToRedisArgs};
use spacegate_model::BoxError;

use super::{ConfigEventType, ConfigType};
pub mod create;
pub mod delete;
pub mod listen;
pub mod retrieve;
pub mod update;

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
pub const CONF_EVENT_CHANNEL: &str = "sg:conf:event";

pub struct Redis<F, P = String> {
    redis_conn_pool: Pool,
    param: P,
    pub format: F,
}

pub trait RedisParam: redis::IntoConnectionInfo + Send + Sync + Clone + Sized {}

impl<T> RedisParam for T where T: redis::IntoConnectionInfo + Send + Sync + Clone + Sized {}

impl<F, P> Redis<F, P>
where
    F: ConfigFormat,
    P: RedisParam,
{
    pub fn new(param: P, format: F) -> BoxResult<Self> {
        let pool = Pool::builder(Manager::new(param.clone())?).build()?;
        Ok(Self {
            redis_conn_pool: pool,
            format,
            param,
        })
    }

    pub async fn get_con(&self) -> BoxResult<Connection> {
        Ok(self.redis_conn_pool.get().await.map_err(Box::new)?)
    }
}

struct RedisConfEvent(pub(crate) ConfigType, pub(crate) ConfigEventType);

impl ToRedisArgs for RedisConfEvent {
    fn write_redis_args<W: ?Sized + RedisWrite>(&self, out: &mut W) {
        out.write_arg_fmt(format!("{}/{}", self.1, self.0));
    }
}

impl FromRedisValue for RedisConfEvent {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        let s: String = redis::FromRedisValue::from_redis_value(v)?;
        match s.parse() {
            Ok(event) => Ok(event),
            Err(e) => Err(std::io::Error::other(format!("fail to parse event {e}")).into()),
        }
    }
}
impl FromStr for RedisConfEvent {
    type Err = BoxError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((method, object)) = s.split_once('/') else {
            return Err("Invalid format".into());
        };
        Ok(Self(ConfigType::from_str(object)?, ConfigEventType::from_str(method)?))
    }
}
