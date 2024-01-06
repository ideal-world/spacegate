use hyper::{Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use spacegate_tower::{helper_layers::filter::FilterRequest, SgResponseExt};
use spacegate_tower::{
    helper_layers::filter::{Filter, FilterRequestLayer},
    SgBoxLayer,
};
use tardis::url::Url;

use spacegate_tower::SgBody;

use crate::{def_plugin, model::SgHttpPathModifier, MakeSgLayer};

/// RedirectFilter defines a filter that redirects a request.
///
/// https://gateway-api.sigs.k8s.io/geps/gep-726/
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RedirectFilter {
    /// Scheme is the scheme to be used in the value of the Location header in the response. When empty, the scheme of the request is used.
    pub scheme: Option<String>,
    /// Hostname is the hostname to be used in the value of the Location header in the response. When empty, the hostname in the Host header of the request is used.
    pub hostname: Option<String>,
    /// Path defines parameters used to modify the path of the incoming request. The modified path is then used to construct the Location header. When empty, the request path is used as-is.
    pub path: Option<SgHttpPathModifier>,
    /// Port is the port to be used in the value of the Location header in the response.
    pub port: Option<u16>,
    /// StatusCode is the HTTP status code to be used in response.
    pub status_code: Option<u16>,
}

impl RedirectFilter {
    fn on_req(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
        let mut url = Url::parse(&req.uri().to_string())
            .map_err(|e| Response::<SgBody>::with_code_message(StatusCode::BAD_REQUEST, format!("[SG.Filter.Redirect] Url parsing error: {}", e)))?;
        if let Some(hostname) = &self.hostname {
            url.set_host(Some(hostname))
                .map_err(|_| Response::<SgBody>::with_code_message(StatusCode::BAD_REQUEST, format!("[SG.Filter.Redirect] Host {hostname} parsing error")))?;
        }
        if let Some(scheme) = &self.scheme {
            url.set_scheme(scheme).map_err(|_| Response::<SgBody>::with_code_message(StatusCode::BAD_REQUEST, format!("[SG.Filter.Redirect] Scheme {scheme} parsing error")))?;
        }
        if let Some(port) = self.port {
            url.set_port(Some(port)).map_err(|_| Response::<SgBody>::with_code_message(StatusCode::BAD_REQUEST, format!("[SG.Filter.Redirect] Port {port} parsing error")))?;
        }
        // todo!();
        // let matched_match_inst = req.context.get_rule_matched();
        // if let Some(new_url) = http_common_modify_path(req.request.get_uri(), &self.path, matched_match_inst.as_ref())? {
        //     req.request.set_uri(new_url);
        // }
        // ctx.set_action(SgRouteFilterRequestAction::Redirect);
        Ok(req)
    }
}

impl Filter for RedirectFilter {
    fn filter(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
        self.on_req(req)
    }
}

pub type RedirectFilterLayer = FilterRequestLayer<RedirectFilter>;
pub type Redirect<S> = FilterRequest<RedirectFilter, S>;

impl MakeSgLayer for RedirectFilter {
    fn make_layer(&self) -> Result<SgBoxLayer, tower::BoxError> {
        let layer = FilterRequestLayer::new(self.clone());
        Ok(SgBoxLayer::new(layer))
    }
}

def_plugin!("redirect", RedirectPlugin, RedirectFilter);
