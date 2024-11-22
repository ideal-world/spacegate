use ipnet::IpNet;

use crate::{
    extension::{user_group::UserGroup, OriginalIpAddr},
    SgRequestExt,
};

impl UserGroup for IpNet {
    fn is_match(&self, req: &crate::SgRequest) -> bool {
        if let Some(OriginalIpAddr(ip)) = req.extract() {
            self.contains(&ip)
        } else {
            false
        }
    }
}
