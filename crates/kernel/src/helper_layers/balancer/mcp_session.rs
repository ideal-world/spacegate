use crate::{extension::PeerAddr, SgRequest};

use super::BalancePolicy;
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
};

#[derive(Debug, Clone)]
pub struct McpSessionHash<H = DefaultHasher> {
    hasher: PhantomData<fn() -> H>,
}

impl Default for McpSessionHash {
    fn default() -> Self {
        Self { hasher: PhantomData }
    }
}

impl<S, H> BalancePolicy<S, SgRequest> for McpSessionHash<H>
where
    H: Hasher + Default,
{
    fn pick<'s>(&self, instances: &'s [S], req: &SgRequest) -> Option<&'s S> {
        if instances.is_empty() {
            return None;
        }
        if instances.len() == 1 {
            return instances.first();
        }

        let mut hasher = H::default();
        if let Some(session_id) = req.headers().get("Mcp-Session-Id") {
            session_id.as_bytes().hash(&mut hasher);
        } else {
            let ip = req.extensions().get::<PeerAddr>()?.0.ip();
            ip.to_canonical().hash(&mut hasher);
        }
        let hash = hasher.finish();
        let index = (hash % instances.len() as u64) as usize;
        instances.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SgBody;
    use hyper::Request;
    use std::net::SocketAddr;

    #[test]
    fn same_mcp_session_id_picks_same_backend() {
        let policy = McpSessionHash::default();
        let instances = ["a", "b", "c"];
        let req_a = request_with_session("session-a");
        let req_a_again = request_with_session("session-a");

        let first = policy.pick(&instances, &req_a);
        let second = policy.pick(&instances, &req_a_again);

        assert_eq!(first, second);
    }

    #[test]
    fn missing_mcp_session_id_falls_back_to_peer_ip() {
        let policy = McpSessionHash::default();
        let instances = ["a", "b", "c"];
        let req_a = request_with_peer("127.0.0.1:1000");
        let req_a_again = request_with_peer("127.0.0.1:2000");

        let first = policy.pick(&instances, &req_a);
        let second = policy.pick(&instances, &req_a_again);

        assert_eq!(first, second);
    }

    fn request_with_session(session: &str) -> SgRequest {
        let mut req = request_with_peer("127.0.0.1:1000");
        req.headers_mut().insert("Mcp-Session-Id", session.parse().expect("valid session header"));
        req
    }

    fn request_with_peer(peer: &str) -> SgRequest {
        let mut req = Request::builder().uri("/mcp").body(SgBody::empty()).expect("request");
        req.extensions_mut().insert(PeerAddr(peer.parse::<SocketAddr>().expect("peer")));
        req
    }
}
