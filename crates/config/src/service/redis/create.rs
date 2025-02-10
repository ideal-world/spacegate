use redis::AsyncCommands;
use spacegate_model::PluginConfig;

use super::{Redis, RedisConfEvent, CONF_EVENT_CHANNEL, CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY};
use crate::service::Create;
use crate::{service::config_format::ConfigFormat, BoxResult};

impl<F> Create for Redis<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn create_config_item_gateway(&self, gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        let _: () = self.get_con().await?.hset(CONF_GATEWAY_KEY, gateway_name, self.format.ser(&gateway)?).await?;
        let event = RedisConfEvent(
            crate::service::ConfigType::Gateway { name: gateway_name.to_string() },
            crate::service::ConfigEventType::Create,
        );
        let _: () = self.get_con().await?.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }

    async fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> BoxResult<()> {
        let _: () = self.get_con().await?.hset(format!("{}{}", CONF_HTTP_ROUTE_KEY, gateway_name), route_name, self.format.ser(&route)?).await?;
        let event = RedisConfEvent(
            crate::service::ConfigType::Route {
                gateway_name: gateway_name.to_string(),
                name: route_name.to_string(),
            },
            crate::service::ConfigEventType::Create,
        );
        let _: () = self.get_con().await?.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }

    async fn create_plugin(&self, id: &crate::model::PluginInstanceId, value: serde_json::Value) -> BoxResult<()> {
        let key = id.to_string();
        let config = serde_json::to_string_pretty(&PluginConfig::new(id.clone(), value))?;
        let _: () = self.get_con().await?.hset("sg:plugin", key, config).await?;
        let event = RedisConfEvent(crate::service::ConfigType::Plugin { id: id.clone() }, crate::service::ConfigEventType::Create);
        let _: () = self.get_con().await?.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }
}
