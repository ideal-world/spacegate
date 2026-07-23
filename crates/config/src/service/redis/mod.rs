use std::str::FromStr;

use crate::{
    service::{config_format::ConfigFormat, decode_stored_plugin_config, encode_stored_plugin_config},
    BoxResult,
};
use deadpool_redis::{Connection, Manager, Pool};
use redis::{FromRedisValue, RedisWrite, ToRedisArgs};
use spacegate_model::{BoxError, PluginConfig, PluginInstanceId, PluginInstanceName};

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

/// 编码 Redis 插件值，统一使用版本化持久化信封。
fn encode_plugin_config_value(config: PluginConfig) -> serde_json::Value {
    encode_stored_plugin_config(config).expect("PluginConfig should serialize to JSON value")
}

/// 解码 Redis 插件值，同时兼容旧版完整 PluginConfig 和裸 spec。
fn decode_plugin_config_value(id: &PluginInstanceId, value: serde_json::Value) -> Result<PluginConfig, BoxError> {
    decode_stored_plugin_config(id, value, true)
}

/// 从新旧完整配置值中提取实例 ID，裸 spec 则回退到 Redis hash 字段名。
fn decode_plugin_config_entry(key: &str, value: serde_json::Value) -> Result<PluginConfig, BoxError> {
    let embedded = if value.get("_spacegate_format").is_some() {
        let mut config = value.clone();
        config.as_object_mut().map(|object| object.remove("_spacegate_format"));
        serde_json::from_value::<PluginConfig>(config).ok().map(|config| config.id)
    } else {
        serde_json::from_value::<PluginConfig>(value.clone()).ok().map(|config| config.id)
    };
    let id = embedded.or_else(|| parse_plugin_config_key(key)).ok_or_else(|| -> BoxError { format!("[SG.Config] Plugin Config id parse error: {key}").into() })?;
    decode_plugin_config_value(&id, value)
}

/// 解析历史 Redis hash 字段名，兼容 named/anon/mono 实例。
fn parse_plugin_config_key(key: &str) -> Option<PluginInstanceId> {
    if let Some(code) = key.strip_suffix("-m") {
        return Some(PluginInstanceId::new(code.to_string(), PluginInstanceName::Mono));
    }
    if let Some((code, uid)) = key.split_once("-a-") {
        if !code.is_empty() && !uid.is_empty() {
            return Some(PluginInstanceId::new(code.to_string(), PluginInstanceName::anon(uid)));
        }
    }
    if let Some((code, name)) = key.split_once("-n-") {
        if !code.is_empty() && !name.is_empty() {
            return Some(PluginInstanceId::new(code.to_string(), PluginInstanceName::named(name)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use spacegate_model::{PluginConfig, PluginInstanceId, PluginInstanceName};

    use super::{decode_plugin_config_value, encode_plugin_config_value};

    fn plugin_id() -> PluginInstanceId {
        PluginInstanceId::new("hai-auth", PluginInstanceName::named("auth-a1"))
    }

    #[test]
    fn plugin_display_name_redis_codec_supports_versioned_and_legacy_values() {
        let id = plugin_id();
        let encoded = encode_plugin_config_value(PluginConfig {
            id: id.clone(),
            display_name: Some("  生产鉴权  ".to_string()),
            spec: json!({ "cache_url": "redis://redis:6379" }),
        });
        let decoded = decode_plugin_config_value(&id, encoded).unwrap();

        assert_eq!(decoded.display_name.as_deref(), Some("生产鉴权"));
        assert_eq!(decoded.spec, json!({ "cache_url": "redis://redis:6379" }));

        let legacy_config = json!({
            "code": "hai-auth",
            "kind": "named",
            "name": "auth-a1",
            "spec": { "legacy": true }
        });
        let decoded = decode_plugin_config_value(&id, legacy_config).unwrap();
        assert_eq!(decoded.display_name, None);
        assert_eq!(decoded.spec, json!({ "legacy": true }));

        let decoded = decode_plugin_config_value(&id, json!({ "raw_legacy": true })).unwrap();
        assert_eq!(decoded.display_name, None);
        assert_eq!(decoded.spec, json!({ "raw_legacy": true }));
    }
}
