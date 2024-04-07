use super::{Redis, RedisConfEvent, CONF_EVENT_CHANNEL, CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY, CONF_PLUGIN_KEY};
use crate::{
    service::{config_format::ConfigFormat, Delete},
    BoxResult,
};
use redis::AsyncCommands;
impl<F> Delete for Redis<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn delete_config_item_gateway(&self, gateway_name: &str) -> BoxResult<()> {
        let mut conn = self.get_con().await?;
        conn.hdel(CONF_GATEWAY_KEY, gateway_name).await?;
        let event = RedisConfEvent(
            crate::service::ConfigType::Gateway { name: gateway_name.to_string() },
            crate::service::ConfigEventType::Delete,
        );
        conn.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<()> {
        self.get_con().await?.hdel(format!("{}{}", CONF_HTTP_ROUTE_KEY, gateway_name), route_name).await?;
        let event = RedisConfEvent(
            crate::service::ConfigType::Route {
                gateway_name: gateway_name.to_string(),
                name: route_name.to_string(),
            },
            crate::service::ConfigEventType::Delete,
        );
        self.get_con().await?.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }

    async fn delete_plugin(&self, id: &crate::model::PluginInstanceId) -> BoxResult<()> {
        self.get_con().await?.hdel(CONF_PLUGIN_KEY, id.to_string()).await?;
        let event = RedisConfEvent(crate::service::ConfigType::Plugin { id: id.clone() }, crate::service::ConfigEventType::Delete);
        self.get_con().await?.publish(CONF_EVENT_CHANNEL, event).await?;
        Ok(())
    }
}
