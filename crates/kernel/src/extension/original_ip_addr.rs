use std::{net::IpAddr, str::FromStr};

use crate::{extractor::OptionalExtract, Extract, SgRequestExt};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
/// Extract original ip address from request
///
/// # Panics
/// ⚠ **WARNING** ⚠
///
/// If peer addr is not settled, it will panic when there's no original ip information from headers.
pub struct OriginalIpAddr(pub IpAddr);

impl OriginalIpAddr {
    pub fn into_inner(self) -> IpAddr {
        self.0
    }
}

impl std::ops::Deref for OriginalIpAddr {
    type Target = IpAddr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl OptionalExtract for OriginalIpAddr {
    fn extract(req: &hyper::Request<crate::SgBody>) -> Option<Self> {
        if let Some(ip) = req.extensions().get::<OriginalIpAddr>().cloned() {
            return Some(ip);
        }
        const X_FORWARDED_FOR: &str = "x-forwarded-for";
        const X_REAL_IP: &str = "x-real-ip";
        fn header_to_ip(header: &hyper::header::HeaderValue) -> Option<IpAddr> {
            let s = header.to_str().ok()?;
            let ip = IpAddr::from_str(s).ok()?;
            Some(ip)
        }
        let ip = req
            .headers()
            .get(X_REAL_IP)
            .and_then(header_to_ip)
            .or_else(|| req.headers().get_all(X_FORWARDED_FOR).iter().next().and_then(header_to_ip))
            .or_else(|| req.extensions().get::<crate::extension::PeerAddr>().map(|peer| peer.0.ip()))?;
        Some(Self(ip))
    }
}
impl Extract for OriginalIpAddr {
    /// # Panics
    /// if peer addr is not settled, it will panic when there's no original ip information from headers.
    fn extract(req: &hyper::Request<crate::SgBody>) -> Self {
        req.extract::<Option<OriginalIpAddr>>().expect("peer addr is not settled")
    }
}
