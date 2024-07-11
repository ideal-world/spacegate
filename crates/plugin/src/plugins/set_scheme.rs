use hyper::{Request, Response};
use serde::{Deserialize, Serialize};

use spacegate_kernel::{helper_layers::function::Inner, SgBody};

use crate::Plugin;
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetSchemePluginConfig {
    /// scheme to set
    pub scheme: String,
}

#[derive(Debug, Clone)]
pub struct SetSchemePlugin {
    pub scheme: hyper::http::uri::Scheme,
}

impl Plugin for SetSchemePlugin {
    const CODE: &'static str = "set-scheme";

    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, spacegate_kernel::BoxError> {
        let (mut parts, body) = req.into_parts();
        let mut p = parts.uri.into_parts();
        p.scheme = Some(self.scheme.clone());
        if p.authority.is_none() {
            p.authority = Some(hyper::http::uri::Authority::from_static(""));
        }
        if p.path_and_query.is_none() {
            p.path_and_query = Some(hyper::http::uri::PathAndQuery::from_static("/"));
        }
        let uri = hyper::Uri::from_parts(p)?;
        parts.uri = uri;
        let req = Request::from_parts(parts, body);
        Ok(inner.call(req).await)
    }

    fn create(config: crate::PluginConfig) -> Result<Self, spacegate_kernel::BoxError> {
        let config = serde_json::from_value::<SetSchemePluginConfig>(config.spec)?;
        let scheme = config.scheme.parse().map_err(|e| format!("parse scheme error: {}", e))?;
        Ok(Self { scheme })
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(SetSchemePlugin, SetSchemePluginConfig);
