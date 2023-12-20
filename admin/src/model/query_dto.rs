use crate::helper::fuzzy_regex;

use tardis::basic::result::TardisResult;
use tardis::regex::Regex;

pub trait Instance {}
pub trait ToInstance<T: Instance> {
    fn to_instance(self) -> TardisResult<T>;
}

#[derive(Default)]
pub struct BackendRefQueryDto {
    pub(crate) names: Option<Vec<String>>,
    pub(crate) namespace: Option<String>,
    pub(crate) hosts: Option<Vec<String>>,
}

impl ToInstance<BackendRefQueryInst> for BackendRefQueryDto {
    fn to_instance(self) -> TardisResult<BackendRefQueryInst> {
        Ok(BackendRefQueryInst {
            names: self.names.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
            namespace: self.namespace.map(fuzzy_regex).transpose()?,
            hosts: self.hosts.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
        })
    }
}

pub struct BackendRefQueryInst {
    pub(crate) names: Option<Vec<Regex>>,
    pub(crate) namespace: Option<Regex>,
    pub(crate) hosts: Option<Vec<Regex>>,
}

impl Instance for BackendRefQueryInst {}

#[derive(Default)]
pub struct GatewayQueryDto {
    pub names: Option<Vec<String>>,
    pub port: Option<u16>,
    pub hostname: Option<String>,
    pub tls_ids: Option<Vec<String>>,
}

impl ToInstance<GatewayQueryInst> for GatewayQueryDto {
    fn to_instance(self) -> TardisResult<GatewayQueryInst> {
        Ok(GatewayQueryInst {
            names: self.names.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
            port: self.port,
            hostname: self.hostname.map(fuzzy_regex).transpose()?,
            tls_ids: self.tls_ids.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
        })
    }
}

pub struct GatewayQueryInst {
    pub names: Option<Vec<Regex>>,
    pub port: Option<u16>,
    pub hostname: Option<Regex>,
    pub tls_ids: Option<Vec<Regex>>,
}
impl Instance for GatewayQueryInst {}

#[derive(Default)]
pub struct SgTlsQueryVO {
    pub names: Option<Vec<String>>,
}

impl ToInstance<SgTlsQueryInst> for SgTlsQueryVO {
    fn to_instance(self) -> TardisResult<SgTlsQueryInst> {
        Ok(SgTlsQueryInst {
            names: self.names.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
        })
    }
}

pub struct SgTlsQueryInst {
    pub names: Option<Vec<Regex>>,
}
impl Instance for SgTlsQueryInst {}

#[derive(Default)]
pub struct PluginQueryDto {
    pub ids: Option<Vec<String>>,
    pub name: Option<String>,
    pub code: Option<String>,
    pub namespace: Option<String>,
    pub target_name: Option<String>,
    pub target_kind: Option<String>,
    pub target_namespace: Option<String>,
}

impl ToInstance<PluginQueryInst> for PluginQueryDto {
    fn to_instance(self) -> TardisResult<PluginQueryInst> {
        Ok(PluginQueryInst {
            ids: self.ids.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
            name: self.name.map(fuzzy_regex).transpose()?,
            code: self.code.map(fuzzy_regex).transpose()?,
            namespace: self.namespace.map(fuzzy_regex).transpose()?,
            target_name: self.target_name.map(fuzzy_regex).transpose()?,
            target_kind: self.target_kind.map(fuzzy_regex).transpose()?,
            target_namespace: self.target_namespace.map(fuzzy_regex).transpose()?,
        })
    }
}

pub struct PluginQueryInst {
    pub ids: Option<Vec<Regex>>,
    pub name: Option<Regex>,
    pub code: Option<Regex>,
    pub namespace: Option<Regex>,
    //todo how to query?
    pub target_name: Option<Regex>,
    pub target_kind: Option<Regex>,
    pub target_namespace: Option<Regex>,
}

impl Instance for PluginQueryInst {}

pub struct HttpRouteQueryDto {
    pub names: Option<Vec<String>>,
    pub gateway_name: Option<String>,
    pub hostnames: Option<Vec<String>>,
    pub filter_ids: Option<Vec<String>>,
}

impl ToInstance<HttpRouteQueryInst> for HttpRouteQueryDto {
    fn to_instance(self) -> TardisResult<HttpRouteQueryInst> {
        Ok(HttpRouteQueryInst {
            names: self.names.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
            gateway_name: self.gateway_name.map(fuzzy_regex).transpose()?,
            hostnames: self.hostnames.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
            filter_ids: self.filter_ids.map(|n| n.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
        })
    }
}

pub struct HttpRouteQueryInst {
    pub names: Option<Vec<Regex>>,
    pub gateway_name: Option<Regex>,
    pub hostnames: Option<Vec<Regex>>,
    pub filter_ids: Option<Vec<Regex>>,
}

impl Instance for HttpRouteQueryInst {}

#[derive(Default)]
pub struct SpacegateInstQueryDto {
    pub(crate) names: Option<Vec<String>>,
}

impl ToInstance<SpacegateInstQueryInst> for SpacegateInstQueryDto {
    fn to_instance(self) -> TardisResult<SpacegateInstQueryInst> {
        Ok(SpacegateInstQueryInst {
            names: self.names.map(|names| names.into_iter().map(fuzzy_regex).collect::<TardisResult<Vec<_>>>()).transpose()?,
        })
    }
}

pub struct SpacegateInstQueryInst {
    pub(crate) names: Option<Vec<Regex>>,
}

impl Instance for SpacegateInstQueryInst {}
