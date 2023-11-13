use crate::model::vo::gateway_vo::{SgGatewayVo, SgListenerVO};
use crate::model::vo::http_route_vo::{SgHttpRouteRuleVO, SgHttpRouteVo};
use crate::model::vo::plugin_vo::SgFilterVO;
use crate::model::vo::Vo;
use kernel_common::helper::k8s_helper::{format_k8s_obj_unique, get_k8s_obj_unique};
use kernel_common::inner_model::gateway::SgParameters;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tardis::basic::result::TardisResult;
use tardis::web::poem_openapi;
use tardis::TardisFuns;

pub trait ToVo<T: Vo> {
    fn to_vo(self) -> TardisResult<T>;
}

#[derive(Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgFilterAddVo {
    pub code: String,
    pub name: Option<String>,
    pub spec: Value,
}

impl ToVo<SgFilterVO> for SgFilterAddVo {
    fn to_vo(self) -> TardisResult<SgFilterVO> {
        Ok(SgFilterVO {
            id: TardisFuns::field.nanoid(),
            code: self.code,
            name: self.name,
            spec: self.spec,
        })
    }
}
