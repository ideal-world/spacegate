#[cfg(feature = "k8s")]
#[derive(Clone)]
pub struct SgSingeFilter {
    pub name: Option<String>,
    pub namespace: String,
    pub filter: crate::k8s_crd::sg_filter::K8sSgFilterSpecFilter,
    pub target_ref: crate::k8s_crd::sg_filter::K8sSgFilterSpecTargetRef,
}

impl From<SgSingeFilter> for crate::k8s_crd::sg_filter::SgFilter {
    fn from(value: SgSingeFilter) -> Self {
        crate::k8s_crd::sg_filter::SgFilter {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: value.name.clone(),
                namespace: Some(value.namespace.clone()),
                ..Default::default()
            },
            spec: crate::k8s_crd::sg_filter::K8sSgFilterSpec {
                filters: vec![value.filter.clone()],
                target_refs: vec![value.target_ref.clone()],
            },
        }
    }
}
