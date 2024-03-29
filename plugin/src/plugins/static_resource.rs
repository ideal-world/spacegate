use std::path::PathBuf;

use hyper::{
    body::Bytes,
    header::{HeaderValue, CONTENT_TYPE},
    Response, StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use spacegate_kernel::{extension::Reflect, helper_layers::filter::Filter, BoxError, SgBody};

use crate::{Plugin, PluginError};

/// StaticResourceConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StaticResourceConfig {
    /// response status code
    pub code: u16,
    /// response content type
    pub content_type: String,
    /// response body
    pub body: BodyEnum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", content = "value")]
pub enum BodyEnum {
    /// json value
    Json(Value),
    /// plain text
    Text(String),
    /// read a static file from file system
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct StaticResource {
    pub code: StatusCode,
    pub content_type: HeaderValue,
    pub body: Bytes,
}

impl Filter for StaticResource {
    fn filter(&self, req: hyper::Request<spacegate_kernel::SgBody>) -> Result<hyper::Request<spacegate_kernel::SgBody>, hyper::Response<spacegate_kernel::SgBody>> {
        let mut resp = Response::builder()
            .header(CONTENT_TYPE, self.content_type.clone())
            .status(self.code)
            .body(SgBody::full(self.body.clone()))
            .map_err(PluginError::internal_error::<StaticResourcePlugin>)?;
        if let Some(reflect) = req.into_parts().0.extensions.remove::<Reflect>() {
            resp.extensions_mut().extend(reflect.into_inner());
        }
        Err(resp)
    }
}

pub struct StaticResourcePlugin {
    pub code: StatusCode,
    pub content_type: HeaderValue,
    pub body: Bytes,
}

impl Plugin for StaticResourcePlugin {
    const CODE: &'static str = "static-resource";
    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        Some(<Self as crate::PluginSchemaExt>::schema())
    }

    async fn call(&self, req: hyper::Request<SgBody>, _inner: spacegate_kernel::helper_layers::function::Inner) -> Result<Response<SgBody>, BoxError> {
        let mut resp = Response::builder()
            .header(CONTENT_TYPE, self.content_type.clone())
            .status(self.code)
            .body(SgBody::full(self.body.clone()))
            .map_err(PluginError::internal_error::<StaticResourcePlugin>)?;
        if let Some(reflect) = req.into_parts().0.extensions.remove::<Reflect>() {
            resp.extensions_mut().extend(reflect.into_inner());
        }
        Ok(resp)
    }

    fn create(config: crate::PluginConfig) -> Result<Self, spacegate_kernel::BoxError> {
        let plugin_config: StaticResourceConfig = serde_json::from_value(config.spec)?;
        let content_type = plugin_config.content_type.clone();
        let content_type = HeaderValue::from_maybe_shared(content_type)?;
        let body = match &plugin_config.body {
            BodyEnum::Json(value) => Bytes::copy_from_slice(value.to_string().as_bytes()),
            BodyEnum::Text(text) => Bytes::copy_from_slice(text.as_bytes()),
            BodyEnum::File(path) => Bytes::copy_from_slice(&std::fs::read(path)?),
        };
        let code = StatusCode::from_u16(plugin_config.code)?;
        Ok(Self { content_type, code, body })
    }
}

#[cfg(feature = "schema")]
crate::schema!(StaticResourcePlugin, StaticResourceConfig);
