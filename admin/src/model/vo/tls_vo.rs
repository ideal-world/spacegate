use crate::constants;
use crate::model::vo::Vo;
use kernel_common::inner_model::gateway::SgTls;

impl Vo for SgTls {
    fn get_vo_type() -> String {
        constants::TLS_CONFIG_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        self.name.clone()
    }
}
