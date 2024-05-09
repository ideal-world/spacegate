use spacegate_model::ext::k8s::crd::sg_filter::K8sSgFilterSpecTargetRef;

pub mod filter_k8s_conv;
pub mod gateway_k8s_conv;
pub mod route_k8s_conv;

pub(crate) trait ToTarget {
    fn to_target_ref(&self) -> K8sSgFilterSpecTargetRef;
}
