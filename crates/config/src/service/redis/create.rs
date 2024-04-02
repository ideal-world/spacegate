use chrono::Utc;
use redis::AsyncCommands;

use super::{Redis, CONF_CHANGE_TRIGGER, CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY};
use crate::service::Create;
use crate::{service::config_format::ConfigFormat, BoxResult};

impl<F> Create for Redis<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn create_config_item_gateway(&self, gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        self.get_con().await?.hset(CONF_GATEWAY_KEY, gateway_name, self.format.ser(&gateway)?).await?;
        let trigger = format!("{}##gateway##create##{gateway_name}##", Utc::now().timestamp());
        self.get_con().await?.set(&format!("{}{}", CONF_CHANGE_TRIGGER, trigger), "").await?;
        Ok(())
    }

    async fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> BoxResult<()> {
        self.get_con().await?.hset(format!("{}{}", CONF_HTTP_ROUTE_KEY, gateway_name), route_name, self.format.ser(&route)?).await?;
        let trigger = format!("{}##httproute##create##{gateway_name}##{route_name}", Utc::now().timestamp());
        self.get_con().await?.set(&format!("{}{}", CONF_CHANGE_TRIGGER, trigger), "").await?;
        Ok(())
    }

    async fn create_plugin(&self, id: &crate::model::PluginInstanceId, value: serde_json::Value) -> BoxResult<()> {
        self.get_con().await?.hset("sg:plugin", id.to_string(), value.to_string()).await?;
        Ok(())
    }
}
