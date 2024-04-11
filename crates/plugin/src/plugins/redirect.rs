use hyper::{Request, Response};
use serde::{Deserialize, Serialize};

use url::Url;

use spacegate_kernel::SgBody;

use crate::{model::SgHttpPathModifier, Plugin};

/// RedirectFilter defines a filter that redirects a request.
///
/// https://gateway-api.sigs.k8s.io/geps/gep-726/
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedirectPlugin {
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

impl Plugin for RedirectPlugin {
    const CODE: &'static str = "redirect";

    async fn call(&self, req: Request<SgBody>, inner: spacegate_kernel::helper_layers::function::Inner) -> Result<Response<SgBody>, spacegate_kernel::BoxError> {
        let mut url = Url::parse(&req.uri().to_string())?;
        if let Some(hostname) = &self.hostname {
            url.set_host(Some(hostname))?;
        }
        if let Some(scheme) = &self.scheme {
            url.set_scheme(scheme).map_err(|_| "fail to set schema")?;
        }
        if let Some(port) = self.port {
            url.set_port(Some(port)).map_err(|_| "fail to set port")?;
        }
        // todo!();
        // let matched_match_inst = req.context.get_rule_matched();
        // if let Some(new_url) = http_common_modify_path(req.request.get_uri(), &self.path, matched_match_inst.as_ref())? {
        //     req.request.set_uri(new_url);
        // }
        // ctx.set_action(SgRouteFilterRequestAction::Redirect);
        Ok(inner.call(req).await)
    }

    fn create(config: crate::PluginConfig) -> Result<Self, spacegate_kernel::BoxError> {
        Ok(serde_json::from_value(config.spec)?)
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

// def_plugin!("redirect", RedirectPlugin, RedirectFilter; #[cfg(feature = "schema")] schema;);
#[cfg(feature = "schema")]
crate::schema!(RedirectPlugin, RedirectPlugin);
