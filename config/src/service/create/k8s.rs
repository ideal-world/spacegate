use k8s_gateway_api::{Gateway, GatewaySpec, GatewayTlsConfig, Listener, SecretObjectReference};
use kube::api::ObjectMeta;

use crate::{constants, k8s_crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef, SgFilterTargetKind}, model::{helper_filter::SgSingeFilter, SgGateway}, service::backend::k8s::K8s, BoxResult};

use super::Create;

impl Create for K8s{
    async fn create_config_item_gateway(&self, gateway_name: &str, gateway: &crate::model::SgGateway) ->BoxResult<()>{
        todo!()
    }

    async fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: &crate::model::SgHttpRoute) -> BoxResult<()>{
        todo!()
    }
}

