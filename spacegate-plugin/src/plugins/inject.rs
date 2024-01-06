use std::sync::Arc;
use std::time::Duration;

use hyper::{header::HeaderName, Request};
use hyper::{Method, Response, Uri};
use serde::{Deserialize, Serialize};
use spacegate_tower::extension::Reflect;
use spacegate_tower::helper_layers::bidirection_filter::{Bdf, BdfLayer, BoxReqFut, BoxRespFut};
use spacegate_tower::service::http_client_service::get_client;
use spacegate_tower::{SgBody, SgResponseExt};
use tower::BoxError;

use crate::{def_plugin, MakeSgLayer};
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterInject {
    pub req_inject_url: Option<String>,
    pub req_timeout: Duration,
    pub resp_inject_url: Option<String>,
    pub resp_timeout: Duration,
}

impl Default for SgFilterInject {
    fn default() -> Self {
        SgFilterInject {
            req_inject_url: None,
            req_timeout: DEFAULT_TIMEOUT,
            resp_inject_url: None,
            resp_timeout: DEFAULT_TIMEOUT,
        }
    }
}
#[derive(Debug, Clone)]
struct InjectRealMethod(pub Method);

#[derive(Debug, Clone)]
struct InjectRealUrl(pub Uri);
const SG_INJECT_REAL_METHOD: &str = "sg-inject-real-method";
const SG_INJECT_REAL_URL: &str = "sg-inject-real-url";
impl SgFilterInject {
    async fn req_filter(&self, mut req: Request<SgBody>) -> Result<Request<SgBody>, BoxError> {
        let real_method = req.method().clone();
        let real_uri = req.uri().clone();
        let reflect = req.extensions_mut().get_mut::<Reflect>().expect("should have reflect extension");
        reflect.insert(InjectRealMethod(real_method.clone()));
        reflect.insert(InjectRealUrl(real_uri.clone()));
        if let Some(req_inject_url) = &self.req_inject_url {
            let (real_parts, real_body) = req.into_parts();
            let inject_request = Request::builder()
                .method(Method::PUT)
                .uri(req_inject_url)
                .header(HeaderName::from_static(SG_INJECT_REAL_METHOD), real_method.as_str())
                .header(HeaderName::from_static(SG_INJECT_REAL_URL), real_uri.to_string())
                .body(real_body)?;
            let raw_extension = real_parts.extensions;
            let mut client = get_client();
            let (resp_part, resp_body) = client.request_timeout(inject_request, self.req_timeout).await.into_parts();
            let mut new_req_headers = resp_part.headers;
            let new_req_method =
                new_req_headers.remove(HeaderName::from_static(SG_INJECT_REAL_METHOD)).map(|m| Method::from_bytes(m.as_bytes())).transpose()?.unwrap_or(real_parts.method);
            #[allow(clippy::unnecessary_to_owned)]
            let new_req_url =
                new_req_headers.remove(HeaderName::from_static(SG_INJECT_REAL_URL)).map(|m| Uri::from_maybe_shared(m.to_owned())).transpose()?.unwrap_or(real_parts.uri);
            let mut new_request = Request::builder().method(new_req_method).uri(new_req_url).version(real_parts.version).body(resp_body)?;
            new_request.extensions_mut().extend(raw_extension);
            *new_request.headers_mut() = new_req_headers.clone();
            Ok(new_request)
        } else {
            Ok(req)
        }
    }

    async fn resp_filter(&self, resp: Response<SgBody>) -> Result<Response<SgBody>, BoxError> {
        if let Some(resp_inject_url) = &self.resp_inject_url {
            let (real_parts, real_body) = resp.into_parts();
            let mut inject_request = Request::builder().method(Method::PUT).uri(resp_inject_url).body(real_body)?;
            if let Some(real_method) = real_parts.extensions.get::<InjectRealMethod>() {
                inject_request.headers_mut().insert(SG_INJECT_REAL_METHOD, real_method.0.as_str().parse()?);
            }
            if let Some(real_url) = real_parts.extensions.get::<InjectRealUrl>() {
                inject_request.headers_mut().insert(SG_INJECT_REAL_URL, real_url.0.to_string().parse()?);
            }
            let mut client = get_client();
            let resp = client.request_timeout(inject_request, self.resp_timeout).await;

            Ok(resp)
        } else {
            Ok(resp)
        }
    }
}

impl Bdf for SgFilterInject {
    type FutureReq = BoxReqFut;

    type FutureResp = BoxRespFut;

    fn on_req(self: Arc<Self>, req: Request<SgBody>) -> Self::FutureReq {
        Box::pin(async move {
            match self.req_filter(req).await {
                Ok(req) => Ok(req),
                Err(e) => Err(Response::<SgBody>::with_code_message(
                    hyper::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("[SG.Filter.Inject] {}", e),
                )),
            }
        })
    }

    fn on_resp(self: Arc<Self>, resp: Response<SgBody>) -> Self::FutureResp {
        Box::pin(async move {
            match self.resp_filter(resp).await {
                Ok(resp) => resp,
                Err(e) => Response::<SgBody>::with_code_message(hyper::StatusCode::INTERNAL_SERVER_ERROR, format!("[SG.Filter.Inject] {}", e)),
            }
        })
    }
}

impl MakeSgLayer for SgFilterInject {
    fn make_layer(&self) -> Result<spacegate_tower::SgBoxLayer, BoxError> {
        let layer = BdfLayer::new(self.clone());
        Ok(spacegate_tower::SgBoxLayer::new(layer))
    }
}

def_plugin!("inject", InjectPlugin, SgFilterInject);
