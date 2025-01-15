use std::net::SocketAddr;

use crate::Extract;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct PeerAddr(pub SocketAddr);

impl Extract for PeerAddr {
    fn extract(req: &hyper::Request<crate::SgBody>) -> Self {
        let peer_addr = req.extensions().get::<PeerAddr>().expect("PeerAddr not found");
        *peer_addr
    }
}
