use std::{convert::Infallible, sync::Arc};

use hyper::{header::HOST, service::Service, StatusCode};
use hyper_util::rt::TokioIo;
use spacegate_kernel::{extension::PeerAddr, service::HyperServiceAdapter, utils::HostAndPort, ArcHyperService, SgRequest, SgResponse, SgResponseExt};

use crate::resolver::MitmCertResolver;
type Executor = hyper_util::rt::TokioExecutor;
type ConnectionBuilder = hyper_util::server::conn::auto::Builder<Executor>;

#[derive(Clone)]
pub struct MitmProxy {
    inner_service: ArcHyperService,
}
#[derive(Debug, Clone)]
pub enum RawScheme {
    Http,
    Https,
}
impl MitmProxy {
    pub fn new(inner: ArcHyperService) -> Self {
        Self { inner_service: inner }
    }
    pub fn as_service(&self) -> ArcHyperService {
        ArcHyperService::new(self.clone())
    }
    pub async fn proxy(&self, mut req: SgRequest) -> SgResponse {
        let scheme = req.uri().scheme_str().unwrap_or("https");
        tracing::debug!(method=%req.method(), uri=%req.uri(), headers=?req.headers(), %scheme, "");
        if scheme == "http" {
            req.extensions_mut().insert(RawScheme::Http);
        } else {
            req.extensions_mut().insert(RawScheme::Https);
        }
        if req.method() == hyper::Method::CONNECT {
            let peer = req.extensions().get::<PeerAddr>().map(|p| p.0).expect("peer addr should be settled");
            let service = self.inner_service.clone();
            let Some(host_and_port) = req.headers().get(HOST).map(HostAndPort::from_header) else {
                return SgResponse::with_code_message(StatusCode::BAD_REQUEST, "missing host header");
            };
            let Ok(host_str) = std::str::from_utf8(host_and_port.host) else {
                return SgResponse::with_code_message(StatusCode::BAD_REQUEST, "invalid host header");
            };
            tracing::info!("connect to {}", host_str);
            let mut tls_config = rustls::ServerConfig::builder().with_no_client_auth().with_cert_resolver(Arc::new(MitmCertResolver::new(host_str)));
            tls_config.ignore_client_order = true;
            tls_config.enable_secret_extraction = true;
            // for potentially configuring nginx without h2 support reason, we only support http/1.1 here
            tls_config.alpn_protocols = vec![b"http/1.1".to_vec()];
            tokio::spawn(async move {
                let req_upgrade = match hyper::upgrade::on(req).await {
                    Ok(req_upgrade) => req_upgrade,
                    Err(e) => {
                        tracing::error!("upgrade request error: {}", e);
                        return;
                    }
                };
                let stream = TokioIo::new(req_upgrade);
                let service = HyperServiceAdapter::new(service, peer);
                let builder = ConnectionBuilder::new(Executor::new());

                let connector = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));
                let Ok(accepted) = connector.accept(stream).await.inspect_err(|e| {
                    tracing::error!("accept tls connection error: {}", e);
                }) else {
                    return;
                };
                let result = builder.serve_connection_with_upgrades(TokioIo::new(accepted), service).await;

                if let Err(e) = result {
                    tracing::error!("serve upgraded http connection error: {}", e);
                }
            });
            SgResponse::with_code_empty(StatusCode::OK)
        } else {
            self.inner_service.call(req).await.expect("infallible")
        }
    }
}

impl Service<SgRequest> for MitmProxy {
    type Response = SgResponse;
    type Error = Infallible;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: SgRequest) -> Self::Future {
        let this = self.clone();
        Box::pin(async move { Ok(this.proxy(req).await) })
    }
}
