use hyper::{Request, Response, Version};
use serde::{Deserialize, Serialize};

use spacegate_kernel::SgBody;

use crate::Plugin;
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SetVersionPluginConfig {
    /// version to set
    pub version: PluginSupportedVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PluginSupportedVersion {
    Http10,
    #[default]
    Http11,
    Http2,
    Http3,
}

#[derive(Debug, Clone, Default)]
pub struct SetVersionPlugin {
    pub version: Version,
}

impl Plugin for SetVersionPlugin {
    const CODE: &'static str = "set-version";

    async fn call(&self, mut req: Request<SgBody>, inner: spacegate_kernel::helper_layers::function::Inner) -> Result<Response<SgBody>, spacegate_kernel::BoxError> {
        *req.version_mut() = self.version;
        Ok(inner.call(req).await)
    }

    fn create(config: crate::PluginConfig) -> Result<Self, spacegate_kernel::BoxError> {
        let version = config.spec.get("version").and_then(|v| v.as_str()).ok_or("version not found")?;
        Ok(Self {
            version: match version.to_uppercase().as_str() {
                "HTTP/1.0" | "HTTP10" => Version::HTTP_10,
                "HTTP/1.1" | "HTTP11" => Version::HTTP_11,
                "HTTP/2.0" | "HTTP2" => Version::HTTP_2,
                "HTTP/3.0" | "HTTP3" => Version::HTTP_3,
                _ => return Err("version not found".into()),
            },
        })
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(SetVersionPlugin, SetVersionPluginConfig);
