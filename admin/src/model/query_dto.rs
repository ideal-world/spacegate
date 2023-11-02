use crate::helper::fuzzy_regex;
use crate::model::base_dto::TargetRefDTO;
#[cfg(feature = "k8s")]
use crate::model::ToFields;
use k8s_openapi::merge_strategies::list::map;
use tardis::basic::result::TardisResult;
use tardis::regex::Regex;

pub trait Instance {}
pub trait ToInstance<T: Instance> {
    fn to_instance(self) -> TardisResult<T>;
}
pub struct BackendRefQueryDto {
    pub(crate) name: Option<String>,
    pub(crate) namespace: Option<String>,
}

#[derive(Default)]
pub struct GatewayQueryDto {
    pub names: Option<Vec<String>>,
    pub port: Option<u16>,
    pub hostname: Option<String>,
}

impl ToInstance<GatewayQueryInst> for GatewayQueryDto {
    fn to_instance(self) -> TardisResult<GatewayQueryInst> {
        Ok(GatewayQueryInst {
            names: self.names.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
            port: self.port,
            hostname: self.hostname.map(fuzzy_regex).transpose()?,
        })
    }
}

pub struct GatewayQueryInst {
    pub names: Option<Vec<Regex>>,
    pub port: Option<u16>,
    pub hostname: Option<Regex>,
}
impl Instance for GatewayQueryInst {}

// #[cfg(feature = "k8s")]
// impl ToFields for GatewayQueryDto {
//     fn to_fields_vec(&self) -> Vec<String> {
//         let mut result = vec![];
//         if let Some(name) = &self.name {
//             result.push(format!("metadata.name={}", name))
//         };
//         if let Some(namespace) = &self.namespace {
//             result.push(format!("metadata.namespace={}", namespace))
//         };
//         result
//     }
// }

#[derive(Default)]
pub struct SgTlsQueryVO {
    pub names: Option<Vec<String>>,
}

impl ToInstance<SgTlsQueryVOInst> for SgTlsQueryVO {
    fn to_instance(self) -> TardisResult<SgTlsQueryVOInst> {
        Ok(SgTlsQueryVOInst {
            names: self.names.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
        })
    }
}

pub struct SgTlsQueryVOInst {
    pub names: Option<Vec<Regex>>,
}
impl Instance for SgTlsQueryVOInst {}

#[derive(Default)]
pub struct PluginQueryDto {
    pub ids: Option<Vec<String>>,
    pub name: Option<String>,
    pub code: Option<String>,
    pub namespace: Option<String>,
    pub target: Option<TargetRefDTO>,
}

// #[cfg(feature = "k8s")]
// impl ToFields for PluginQueryDto {
//     fn to_fields_vec(&self) -> Vec<String> {
//         let mut result = vec![];
//         if let Some(name) = &self.name {
//             result.push(format!("metadata.name={}", name))
//         };
//         result
//     }
// }
