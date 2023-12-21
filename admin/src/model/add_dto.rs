use crate::model::vo::plugin_vo::SgFilterVo;
use crate::model::vo::Vo;

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
    pub enable: Option<bool>,
    pub spec: Value,
}

impl ToVo<SgFilterVo> for SgFilterAddVo {
    fn to_vo(self) -> TardisResult<SgFilterVo> {
        Ok(SgFilterVo {
            id: TardisFuns::field.nanoid(),
            code: self.code,
            name: self.name,
            spec: self.spec,
            enable: self.enable.unwrap_or(true),
        })
    }
}
