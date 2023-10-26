use crate::model::base_dto::TargetRefDTO;
#[cfg(feature = "k8s")]
use crate::model::ToFields;

pub struct BackendRefQueryDto {
    pub(crate) name: Option<String>,
    pub(crate) namespace: Option<String>,
}

#[derive(Default)]
pub struct GatewayQueryDto {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub port: Option<u16>,
    pub hostname: Option<String>,
}
pub struct SgTlsConfigQueryVO {}
#[cfg(feature = "k8s")]
impl ToFields for GatewayQueryDto {
    fn to_fields_vec(&self) -> Vec<String> {
        let mut result = vec![];
        if let Some(name) = &self.name {
            result.push(format!("metadata.name={}", name))
        };
        if let Some(namespace) = &self.namespace {
            result.push(format!("metadata.namespace={}", namespace))
        };
        result
    }
}

#[derive(Default)]
pub struct PluginQueryDto {
    pub ids: Option<Vec<String>>,
    pub name: Option<String>,
    pub code: Option<String>,
    pub namespace: Option<String>,
    pub target: Option<TargetRefDTO>,
}

#[cfg(feature = "k8s")]
impl ToFields for PluginQueryDto {
    fn to_fields_vec(&self) -> Vec<String> {
        let mut result = vec![];
        if let Some(name) = &self.name {
            result.push(format!("metadata.name={}", name))
        };
        result
    }
}
