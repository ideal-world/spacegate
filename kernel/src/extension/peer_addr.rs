use std::net::SocketAddr;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct PeerAddr(pub SocketAddr);
