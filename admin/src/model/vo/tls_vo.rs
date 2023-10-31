use crate::constants;
use crate::model::vo::Vo;
use serde::{Deserialize, Serialize};
use tardis::web::poem_openapi;

/// SgTlsVO describes a TLS configuration.
/// unique by id
//todo 直接引用SgTls
#[derive(Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgTlsVO {
    pub name: String,
    pub key: String,
    pub cert: String,
}

impl Vo for SgTlsVO {
    fn get_vo_type() -> String {
        constants::TLS_CONFIG_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        self.name.clone()
    }
}
