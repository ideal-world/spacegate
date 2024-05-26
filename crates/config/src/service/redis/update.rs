use super::{Redis, RedisConfEvent, CONF_EVENT_CHANNEL, CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY, CONF_PLUGIN_KEY};
use crate::{
    service::{config_format::ConfigFormat, Update},
    BoxResult,
};
use redis::AsyncCommands as _;

impl<F> Update for Redis<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn update_config_item_gateway(&self, gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        self.get_con().await?.hset(CONF_GATEWAY_KEY, gateway_name, self.format.ser(&gateway)?).await?;
        let event = RedisConfEvent(
            crate::service::ConfigType::Gateway { name: gateway_name.to_string() },
            crate::service::ConfigEventType::Update,
        );
        self.get_con().await?.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }

    async fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> BoxResult<()> {
        self.get_con().await?.hset(format!("{}{}", CONF_HTTP_ROUTE_KEY, gateway_name), route_name, self.format.ser(&route)?).await?;
        let event = RedisConfEvent(
            crate::service::ConfigType::Route {
                gateway_name: gateway_name.to_string(),
                name: route_name.to_string(),
            },
            crate::service::ConfigEventType::Update,
        );
        self.get_con().await?.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }

    async fn update_plugin(&self, id: &crate::model::PluginInstanceId, value: serde_json::Value) -> BoxResult<()> {
        self.get_con().await?.hset(CONF_PLUGIN_KEY, id.to_string(), value.to_string()).await?;
        let event = RedisConfEvent(crate::service::ConfigType::Plugin { id: id.clone() }, crate::service::ConfigEventType::Update);
        self.get_con().await?.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }
}
