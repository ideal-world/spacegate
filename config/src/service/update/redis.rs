use chrono::Utc;
use redis::AsyncCommands as _
;

use crate::{
    service::{
        backend::redis::{Redis, CONF_CHANGE_TRIGGER, CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY},
        config_format::ConfigFormat,
    },
    BoxResult,
};

use super::Update;

impl<F> Update for Redis<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn update_config_item_gateway(&self, gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        self.get_con().await?.hset(CONF_GATEWAY_KEY, gateway_name, self.format.ser(&gateway)?).await?;
        let trigger = format!("{}##gateway##update##{gateway_name}##", Utc::now().timestamp());
        self.get_con().await?.set(&format!("{}{}", CONF_CHANGE_TRIGGER, trigger), "").await?;
        Ok(())
    }

    async fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> BoxResult<()> {
        self.get_con().await?.hset(format!("{}{}", CONF_HTTP_ROUTE_KEY, gateway_name), route_name, self.format.ser(&route)?).await?;
        let trigger = format!("{}##httproute##update##{gateway_name}##{route_name}", Utc::now().timestamp());
        self.get_con().await?.set(&format!("{}{}", CONF_CHANGE_TRIGGER, trigger), "").await?;
        Ok(())
    }
}
