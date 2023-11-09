use std::hash::{Hash, Hasher};

#[cfg(feature = "k8s")]
#[derive(Clone)]
pub struct SgSingeFilter {
    pub name: Option<String>,
    pub namespace: String,
    pub filter: crate::k8s_crd::sg_filter::K8sSgFilterSpecFilter,
    pub target_ref: crate::k8s_crd::sg_filter::K8sSgFilterSpecTargetRef,
}

impl Hash for SgSingeFilter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.namespace.hash(state);
        self.filter.code.hash(state);
        self.target_ref.kind.hash(state);
        self.target_ref.name.hash(state);
        self.target_ref.namespace.hash(state);
    }
}

impl PartialEq<Self> for SgSingeFilter {
    fn eq(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.filter.code == other.filter.code
            && self.target_ref.kind == other.target_ref.kind
            && self.target_ref.name == other.target_ref.name
            && self.target_ref.namespace.as_ref().unwrap_or(&"default".to_string()) == other.target_ref.namespace.as_ref().unwrap_or(&"default".to_string())
    }
}

impl Eq for SgSingeFilter {}

#[cfg(feature = "k8s")]
impl SgSingeFilter {
    pub fn to_sg_filter(&self) -> crate::k8s_crd::sg_filter::SgFilter {
        crate::k8s_crd::sg_filter::SgFilter {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: self.name.clone(),
                namespace: Some(self.namespace.clone()),
                ..Default::default()
            },
            spec: crate::k8s_crd::sg_filter::K8sSgFilterSpec {
                filters: vec![self.filter.clone()],
                target_refs: vec![self.target_ref.clone()],
            },
        }
    }
}
