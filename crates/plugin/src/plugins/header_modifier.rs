use std::{collections::HashMap, sync::Arc};

use hyper::header::HeaderValue;
use hyper::{header::HeaderName, HeaderMap};
use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use spacegate_kernel::helper_layers::function::Inner;

use spacegate_kernel::{BoxError, SgBody};

use crate::Plugin;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SgFilterHeaderModifierKind {
    #[default]
    Request,
    Response,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]

pub struct SgFilterHeaderModifier {
    pub kind: SgFilterHeaderModifierKind,
    pub sets: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

pub struct HeaderModifierPlugin {
    request: Arc<Filter>,
    response: Arc<Filter>,
}

impl Plugin for HeaderModifierPlugin {
    const CODE: &'static str = "header-modifier";
    fn create(config: crate::PluginConfig) -> Result<Self, spacegate_kernel::BoxError> {
        let plugin_config = serde_json::from_value::<SgFilterHeaderModifier>(config.spec)?;
        let mut sets = HeaderMap::new();
        if let Some(set) = &plugin_config.sets {
            for (k, v) in set.iter() {
                sets.insert(HeaderName::from_bytes(k.as_bytes())?, HeaderValue::from_bytes(v.as_bytes())?);
            }
        }
        let mut remove = Vec::new();
        if let Some(r) = &plugin_config.remove {
            for k in r {
                remove.push(k.parse()?);
            }
        }
        let filter = Filter { sets, remove };
        Ok(match plugin_config.kind {
            SgFilterHeaderModifierKind::Request => Self {
                request: Arc::new(filter),
                response: Arc::new(Filter::default()),
            },
            SgFilterHeaderModifierKind::Response => Self {
                request: Arc::new(Filter::default()),
                response: Arc::new(filter),
            },
        })
    }
    async fn call(&self, mut req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        for (k, v) in &self.request.sets {
            req.headers_mut().append(k, v.clone());
        }
        for k in &self.request.remove {
            req.headers_mut().remove(k);
        }
        let mut resp = inner.call(req).await;
        for (k, v) in &self.response.sets {
            resp.headers_mut().append(k, v.clone());
        }
        for k in &self.response.remove {
            resp.headers_mut().remove(k);
        }
        Ok(resp)
    }
}

#[derive(Clone, Default, Debug)]
struct Filter {
    pub sets: HeaderMap,
    pub remove: Vec<HeaderName>,
}

#[cfg(feature = "schema")]
crate::schema!(HeaderModifierPlugin, SgFilterHeaderModifier);
