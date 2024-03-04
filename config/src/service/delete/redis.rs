use chrono::Utc;
use redis::AsyncCommands;

use crate::{
    service::{
        backend::redis::{Redis, CONF_CHANGE_TRIGGER, CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY},
        config_format::ConfigFormat,
    },
    BoxResult,
};

impl<F> super::Delete for Redis<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn delete_config_item_gateway(&self, gateway_name: &str) -> BoxResult<()> {
        self.get_con().await?.hdel(CONF_GATEWAY_KEY, gateway_name).await?;
        let trigger = format!("{}##gateway##delete##{gateway_name}##", Utc::now().timestamp());
        self.get_con().await?.set(&format!("{}{}", CONF_CHANGE_TRIGGER, trigger), "").await?;
        Ok(())
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<()> {
        self.get_con().await?.hdel(format!("{}{}", CONF_HTTP_ROUTE_KEY, gateway_name), route_name).await?;
        let trigger = format!("{}##httproute##delete##{gateway_name}##{route_name}", Utc::now().timestamp());
        self.get_con().await?.set(&format!("{}{}", CONF_CHANGE_TRIGGER, trigger), "").await?;
        Ok(())
    }
}
