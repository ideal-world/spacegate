#[derive(Clone)]
pub struct SgSingeFilter {
    pub name: String,
    pub namespace: String,
    pub filter: super::crd::sg_filter::K8sSgFilterSpecFilter,
    pub target_ref: Option<super::crd::sg_filter::K8sSgFilterSpecTargetRef>,
}

impl From<SgSingeFilter> for super::crd::sg_filter::SgFilter {
    fn from(value: SgSingeFilter) -> Self {
        super::crd::sg_filter::SgFilter {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: if value.name.is_empty() {
                    Some(value.filter.code.clone())
                } else {
                    Some(value.name.clone())
                },
                namespace: Some(value.namespace.clone()),
                ..Default::default()
            },
            spec: super::crd::sg_filter::K8sSgFilterSpec {
                filters: vec![value.filter.clone()],
                target_refs: if let Some(target_ref) = value.target_ref { vec![target_ref] } else { vec![] },
            },
        }
    }
}

/// see [crate::ext::k8s::crd::K8sSgFilterSpecTargetRef].kind
/// [ParentReference].kind
pub enum SgTargetKind {
    Gateway,
    Httproute,
    Httpspaceroute,
}

impl From<SgTargetKind> for String {
    fn from(value: SgTargetKind) -> Self {
        match value {
            SgTargetKind::Gateway => "Gateway".to_string(),
            SgTargetKind::Httproute => "HTTPRoute".to_string(),
            SgTargetKind::Httpspaceroute => "HTTPSpaceroute".to_string(),
        }
    }
}

pub enum BackendObjectRefKind {
    Service,
    ExternalHttp,
    ExternalHttps,
    File,
}

impl From<BackendObjectRefKind> for String {
    fn from(value: BackendObjectRefKind) -> Self {
        match value {
            BackendObjectRefKind::Service => "Service".to_string(),
            BackendObjectRefKind::ExternalHttp => "ExternalHttp".to_string(),
            BackendObjectRefKind::ExternalHttps => "ExternalHttps".to_string(),
            BackendObjectRefKind::File => "File".to_string(),
        }
    }
}

impl From<String> for BackendObjectRefKind {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Service" => BackendObjectRefKind::Service,
            "ExternalHttp" => BackendObjectRefKind::ExternalHttp,
            "ExternalHttps" => BackendObjectRefKind::ExternalHttps,
            "File" => BackendObjectRefKind::File,
            _ => BackendObjectRefKind::Service,
        }
    }
}

impl From<BackendObjectRefKind> for Option<String> {
    fn from(value: BackendObjectRefKind) -> Self {
        Some(value.into())
    }
}
