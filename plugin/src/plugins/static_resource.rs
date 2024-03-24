use std::path::PathBuf;

use hyper::{
    body::Bytes,
    header::{HeaderValue, CONTENT_TYPE},
    Response, StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use spacegate_kernel::{
    extension::Reflect,
    helper_layers::filter::{Filter, FilterRequestLayer},
    SgBody,
};

use crate::{instance::PluginInstance, MakeSgLayer, Plugin, PluginError};

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

impl MakeSgLayer for StaticResourceConfig {
    fn make_layer(&self) -> spacegate_kernel::BoxResult<spacegate_kernel::SgBoxLayer> {
        let content_type = self.content_type.clone();
        let content_type = HeaderValue::from_maybe_shared(content_type)?;
        let body = match &self.body {
            BodyEnum::Json(value) => Bytes::copy_from_slice(value.to_string().as_bytes()),
            BodyEnum::Text(text) => Bytes::copy_from_slice(text.as_bytes()),
            BodyEnum::File(path) => Bytes::copy_from_slice(&std::fs::read(path)?),
        };
        let code = StatusCode::from_u16(self.code)?;
        Ok(spacegate_kernel::SgBoxLayer::new(FilterRequestLayer::new(StaticResource { content_type, code, body })))
    }
}

pub struct StaticResourcePlugin {}

impl Plugin for StaticResourcePlugin {
    const CODE: &'static str = "static-resource";
    fn create(config: crate::PluginConfig) -> Result<PluginInstance, spacegate_kernel::BoxError> {
        let make_config: StaticResourceConfig = serde_json::from_value(config.spec.clone())?;
        Ok(PluginInstance::new::<Self, _>(config, move || {
            let content_type = make_config.content_type.clone();
            let content_type = HeaderValue::from_maybe_shared(content_type)?;
            let body = match &make_config.body {
                BodyEnum::Json(value) => Bytes::copy_from_slice(value.to_string().as_bytes()),
                BodyEnum::Text(text) => Bytes::copy_from_slice(text.as_bytes()),
                BodyEnum::File(path) => Bytes::copy_from_slice(&std::fs::read(path)?),
            };
            let code = StatusCode::from_u16(make_config.code)?;
            Ok(spacegate_kernel::SgBoxLayer::new(FilterRequestLayer::new(StaticResource { content_type, code, body })))
        }))
    }
    // type MakeLayer = StaticResourceConfig;

    // fn create(_id: Option<String>, value: Value) -> Result<Self::MakeLayer, spacegate_kernel::BoxError> {
    //     let config = serde_json::from_value(value)?;
    //     Ok(config)
    // }
}

#[cfg(feature = "schema")]
crate::schema!(StaticResourcePlugin);
