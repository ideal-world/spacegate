use std::net::IpAddr;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use spacegate_kernel::{
    extension::{IsEastWestTraffic, OriginalIpAddr},
    SgRequestExt as _,
};

use crate::{schema, Plugin};

#[cfg(feature = "schema")]
schema!(EastWestTrafficWhiteListPlugin, EastWestTrafficWhiteListConfig);

#[derive(Debug, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct EastWestTrafficWhiteListConfig {
    pub ip_list: Vec<String>,
}

pub struct EastWestTrafficWhiteListPlugin {
    pub ip_list: Vec<IpNet>,
}

impl From<EastWestTrafficWhiteListConfig> for EastWestTrafficWhiteListPlugin {
    fn from(value: EastWestTrafficWhiteListConfig) -> Self {
        let mut ip_list = vec![];
        let nets: Vec<IpNet> = value.ip_list.iter().filter_map(|p| p.parse().or(p.parse::<IpAddr>().map(IpNet::from)).map_err(|e| {}).ok()).collect();
        for net in IpNet::aggregate(&nets) {
            ip_list.push(net)
        }

        EastWestTrafficWhiteListPlugin { ip_list }
    }
}

impl EastWestTrafficWhiteListPlugin {
    fn match_(&self, ip: &IpAddr) -> bool {
        self.ip_list.iter().any(|net| net.contains(ip))
    }
}

impl Plugin for EastWestTrafficWhiteListPlugin {
    const CODE: &'static str = "east-west-traffic-white-list";

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }

    fn create(plugin_config: spacegate_model::PluginConfig) -> Result<Self, spacegate_kernel::BoxError> {
        let plugin_config = serde_json::from_value::<EastWestTrafficWhiteListConfig>(plugin_config.spec)?;
        return Ok(plugin_config.into());
    }

    async fn call(
        &self,
        mut req: spacegate_kernel::SgRequest,
        inner: spacegate_kernel::helper_layers::function::Inner,
    ) -> Result<spacegate_kernel::SgResponse, spacegate_kernel::BoxError> {
        let original_addr = req.extract::<OriginalIpAddr>().into_inner();
        if self.match_(&original_addr) {
            req.extensions_mut().insert(IsEastWestTraffic::new(true));
        }
        Ok(inner.call(req).await)
    }
}
