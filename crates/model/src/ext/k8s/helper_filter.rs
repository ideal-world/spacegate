use std::hash::{Hash, Hasher};

use crate::constants;

#[derive(Clone)]
pub struct SgSingeFilter {
    pub name: Option<String>,
    pub namespace: String,
    pub filter: super::crd::sg_filter::K8sSgFilterSpecFilter,
    pub target_ref: super::crd::sg_filter::K8sSgFilterSpecTargetRef,
}

impl PartialEq for SgSingeFilter {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.namespace == other.namespace
            && self.filter.code == other.filter.code
            && self.target_ref.kind == other.target_ref.kind
            && self.target_ref.name == other.target_ref.name
            && self.target_ref.namespace.as_ref().unwrap_or(&constants::DEFAULT_NAMESPACE.to_string())
                == other.target_ref.namespace.as_ref().unwrap_or(&constants::DEFAULT_NAMESPACE.to_string())
    }
}

impl Eq for SgSingeFilter {}

impl Hash for SgSingeFilter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.namespace.hash(state);
        self.target_ref.kind.hash(state);
        self.target_ref.name.hash(state);
        self.target_ref.namespace.hash(state);
    }
}

impl From<SgSingeFilter> for super::crd::sg_filter::SgFilter {
    fn from(value: SgSingeFilter) -> Self {
        super::crd::sg_filter::SgFilter {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: value.name.clone(),
                namespace: Some(value.namespace.clone()),
                ..Default::default()
            },
            spec: super::crd::sg_filter::K8sSgFilterSpec {
                filters: vec![value.filter.clone()],
                target_refs: vec![value.target_ref.clone()],
            },
        }
    }
}
