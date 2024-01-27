use crate::{extension::Reflect, SgBody, SgResponseExt};
use futures_util::{Future, FutureExt};

use hyper::StatusCode;
use hyper::{Request, Response};
use hyper_rustls::HttpsConnector;
use hyper_rustls::{ConfigBuilderExt, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use std::{
    collections::HashMap,
    convert::Infallible,
    mem,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio_rustls::rustls::{self, client::danger::ServerCertVerifier, SignatureScheme};
use tower_service::Service;

#[derive(Debug, Clone)]
pub struct NoCertificateVerification {}
impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

fn get_rustls_config_dangerous() -> rustls::ClientConfig {
    let store = rustls::RootCertStore::empty();
    let mut config = rustls::ClientConfig::builder().with_root_certificates(store).with_no_client_auth();
    // completely disable cert-verification
    let mut dangerous_config = rustls::ClientConfig::dangerous(&mut config);
    dangerous_config.set_certificate_verifier(Arc::new(NoCertificateVerification {}));
    config
}

pub fn get_client() -> SgHttpClient {
    unsafe { &GLOBAL }.get_or_init(Default::default).get_default()
}

pub struct ClientRepo {
    default: SgHttpClient,
    repo: Mutex<HashMap<String, SgHttpClient>>,
}

impl Default for ClientRepo {
    fn default() -> Self {
        let config = get_rustls_config_dangerous();
        let default = SgHttpClient::new(config);
        Self {
            default,
            repo: Default::default(),
        }
    }
}

static mut GLOBAL: OnceLock<ClientRepo> = OnceLock::new();
impl ClientRepo {
    pub fn get(&self, code: &str) -> Option<SgHttpClient> {
        self.repo.lock().expect("failed to lock client repo").get(code).cloned()
    }
    pub fn get_or_default(&self, code: &str) -> SgHttpClient {
        self.get(code).unwrap_or_else(|| self.default.clone())
    }
    pub fn get_default(&self) -> SgHttpClient {
        self.default.clone()
    }
    pub fn register(&self, code: &str, client: SgHttpClient) {
        self.repo.lock().expect("failed to lock client repo").insert(code.to_string(), client);
    }
    pub fn set_default(&mut self, client: SgHttpClient) {
        self.default = client;
    }
    pub fn global() -> &'static Self {
        unsafe { &GLOBAL }.get_or_init(Default::default)
    }

    /// # Safety
    /// This function is not thread safe, it should be called before any other thread is spawned.
    pub unsafe fn set_global_default(client: SgHttpClient) {
        GLOBAL.get_or_init(Default::default);
        GLOBAL.get_mut().expect("global not set").set_default(client);
    }
}

pub struct SgHttpClientConfig {
    pub tls_config: rustls::ClientConfig,
}

#[derive(Debug, Clone)]
pub struct SgHttpClient {
    inner: Client<HttpsConnector<HttpConnector>, SgBody>,
}

impl Default for SgHttpClient {
    fn default() -> Self {
        Self::new(rustls::ClientConfig::builder().with_native_roots().expect("failed to init rustls config").with_no_client_auth())
    }
}

impl SgHttpClient {
    pub fn new(tls_config: rustls::ClientConfig) -> Self {
        SgHttpClient {
            inner: Client::builder(TokioExecutor::new()).build(HttpsConnectorBuilder::new().with_tls_config(tls_config).https_or_http().enable_http1().build()),
        }
    }
    pub fn new_dangerous() -> Self {
        let config = get_rustls_config_dangerous();
        Self::new(config)
    }
    pub async fn request(&mut self, mut req: Request<SgBody>) -> Response<SgBody> {
        let reflect = req.extensions_mut().remove::<Reflect>();
        match self.inner.request(req).await.map_err(Response::internal_error) {
            Ok(mut response) => {
                if let Some(reflect) = reflect {
                    response.extensions_mut().extend(reflect.into_inner());
                }
                response.map(SgBody::new)
            }
            Err(err) => err,
        }
    }
    pub async fn request_timeout(&mut self, req: Request<SgBody>, timeout: Duration) -> Response<SgBody> {
        let fut = self.request(req);
        let resp = tokio::time::timeout(timeout, fut).await;
        match resp {
            Ok(resp) => resp,
            Err(_) => Response::with_code_message(StatusCode::GATEWAY_TIMEOUT, "request timeout"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[tokio::test]
    async fn test_client() {
        let mut client = get_client();
        let req = Request::builder().uri("https://www.baidu.com").body(SgBody::empty()).unwrap();
        let resp = client.request(req).await;
        let (part, body) = resp.into_parts();
        let body = body.dump().await.unwrap();
        let dumped = body.get_dumped().expect("no body");
        println!("{part:?}, {}", String::from_utf8_lossy(dumped));
    }
}
