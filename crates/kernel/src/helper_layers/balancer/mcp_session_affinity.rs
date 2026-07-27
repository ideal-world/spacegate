use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use hyper::Response;
use crate::{extension::PeerAddr, SgBody, SgRequest};
/// Process-local MCP session to backend-index mapping with bounded TTL retention.
#[derive(Debug, Clone)]
pub struct McpSessionAffinityBalancer {
    sessions: Arc<Mutex<HashMap<String, SessionBackend>>>,
    capacity: usize,
    ttl: Duration,
}

/// Stored backend selection and its expiration deadline.
#[derive(Debug, Clone, Copy)]
struct SessionBackend {
    backend_index: usize,
    expires_at: Instant,
}

impl Default for McpSessionAffinityBalancer {
    fn default() -> Self {
        Self::with_limits(1024, Duration::from_secs(30 * 60))
    }
}

impl McpSessionAffinityBalancer {
    /// Creates a process-local affinity map with explicit capacity and TTL bounds.
    pub fn with_limits(capacity: usize, ttl: Duration) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            capacity,
            ttl,
        }
    }

    /// Records the backend that emitted an MCP session ID.
    pub fn record_session(&self, session_id: &str, backend_index: usize) {
        let Ok(mut sessions) = self.sessions.lock() else {
            return;
        };
        let now = Instant::now();
        sessions.retain(|_, entry| entry.expires_at > now);
        if !sessions.contains_key(session_id) && sessions.len() >= self.capacity {
            if let Some(oldest) = sessions.iter().min_by_key(|(_, entry)| entry.expires_at).map(|(key, _)| key.clone()) {
                sessions.remove(&oldest);
            }
        }
        if self.capacity > 0 {
            sessions.insert(
                session_id.to_string(),
                SessionBackend {
                    backend_index,
                    expires_at: now + self.ttl,
                },
            );
        }
    }

    /// Removes a session mapping after successful session termination.
    pub fn remove_session(&self, session_id: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(session_id);
        }
    }

    /// Selects a backend index by recorded session, then session hash, then peer IP hash.
    pub fn select_backend_index(&self, session_id: Option<&str>, peer_ip: Option<IpAddr>, backend_count: usize) -> Option<usize> {
        if backend_count == 0 {
            return None;
        }
        if backend_count == 1 {
            return Some(0);
        }
        if let Some(session_id) = session_id {
            if let Ok(mut sessions) = self.sessions.lock() {
                let now = Instant::now();
                sessions.retain(|_, entry| entry.expires_at > now);
                if let Some(entry) = sessions.get(session_id) {
                    if entry.backend_index < backend_count {
                        return Some(entry.backend_index);
                    }
                }
            }
            return Some(hash_index(session_id, backend_count));
        }
        peer_ip.map(|ip| hash_index(&ip.to_canonical(), backend_count))
    }
}

/// Balances MCP requests and records response-created sessions against the selected backend index.
#[derive(Debug, Clone)]
pub struct McpSessionAffinityService<S> {
    /// Process-local session affinity state for this route instance.
    pub affinity: McpSessionAffinityBalancer,
    /// Backend services addressed by stable vector index.
    pub instances: Vec<S>,
    /// Fallback used when no backend is configured or peer information is absent.
    pub fallback: S,
}

impl<S> McpSessionAffinityService<S> {
    /// Creates a fresh affinity map for a newly compiled route.
    pub fn new(instances: Vec<S>, fallback: S) -> Self {
        Self {
            affinity: McpSessionAffinityBalancer::default(),
            instances,
            fallback,
        }
    }
}

impl<S> hyper::service::Service<SgRequest> for McpSessionAffinityService<S>
where
    S: hyper::service::Service<SgRequest, Response = Response<SgBody>, Error = std::convert::Infallible> + Send + Sync + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<SgBody>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    /// Selects by session affinity and records successful response session IDs before returning downstream.
    fn call(&self, request: SgRequest) -> Self::Future {
        let session_id = request.headers().get("Mcp-Session-Id").and_then(|value| value.to_str().ok()).map(ToOwned::to_owned);
        let peer_ip = request.extensions().get::<PeerAddr>().map(|peer| peer.0.ip());
        let terminates_session = request.method() == hyper::Method::DELETE;
        let index = self.affinity.select_backend_index(session_id.as_deref(), peer_ip, self.instances.len());
        let affinity = self.affinity.clone();
        let future = match index {
            Some(index) => self.instances[index].call(request),
            None => self.fallback.call(request),
        };
        Box::pin(async move {
            let response = future.await?;
            if response.status().is_success() {
                if terminates_session {
                    if let Some(session_id) = session_id.as_deref() {
                        affinity.remove_session(session_id);
                    }
                }
                if let (Some(index), Some(response_session)) = (index, response.headers().get("Mcp-Session-Id").and_then(|value| value.to_str().ok())) {
                    affinity.record_session(response_session, index);
                }
            }
            Ok(response)
        })
    }
}

/// Hashes one affinity key into a valid backend index.
fn hash_index(value: &(impl Hash + ?Sized), backend_count: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    (hasher.finish() % backend_count as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::{McpSessionAffinityBalancer, McpSessionAffinityService};
    use crate::{backend_service::ArcHyperService, SgBody};
    use hyper::{Request, Response};
    use std::convert::Infallible;

    /// Initialization response session IDs must pin subsequent requests to the backend that created them.
    #[test]
    fn initialization_response_affinity_overrides_session_hash() {
        let affinity = McpSessionAffinityBalancer::with_limits(16, std::time::Duration::from_secs(60));

        affinity.record_session("session-1", 0);

        assert_eq!(affinity.select_backend_index(Some("session-1"), None, 2), Some(0));
    }

    /// A response-created session must select its creating backend on the next request.
    #[tokio::test]
    async fn response_session_header_pins_follow_up_request_to_selected_backend() {
        let backend_a = ArcHyperService::new(hyper::service::service_fn(|_request: Request<SgBody>| async move {
            Ok::<_, Infallible>(Response::builder().header("Mcp-Session-Id", "session-1").body(SgBody::full("a")).expect("response"))
        }));
        let backend_b = ArcHyperService::new(hyper::service::service_fn(|_request: Request<SgBody>| async move {
            Ok::<_, Infallible>(Response::new(SgBody::full("b")))
        }));
        let service = McpSessionAffinityService::new(vec![backend_a.clone(), backend_b.clone()], backend_b);
        service.affinity.record_session("bootstrap", 0);
        let init = Request::builder().header("Mcp-Session-Id", "bootstrap").body(SgBody::empty()).expect("init request");

        let response = hyper::service::Service::call(&service, init).await.expect("init response");
        assert_eq!(response.headers().get("Mcp-Session-Id").and_then(|value| value.to_str().ok()), Some("session-1"));
        let follow_up = Request::builder().header("Mcp-Session-Id", "session-1").body(SgBody::empty()).expect("follow-up request");

        assert_eq!(service.affinity.select_backend_index(Some("session-1"), None, 2), Some(0));
        let response = hyper::service::Service::call(&service, follow_up).await.expect("follow-up response");
        assert_eq!(response.into_body().get_dumped().expect("body"), "a");
    }
}
