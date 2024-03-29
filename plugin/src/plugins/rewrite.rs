use hyper::header::{HeaderValue, HOST};
use hyper::{Request, Response, Uri};
use serde::{Deserialize, Serialize};
use spacegate_kernel::extension::MatchedSgRouter;
use spacegate_kernel::helper_layers::filter::{Filter, FilterRequest, FilterRequestLayer};
use spacegate_kernel::{SgBody, SgBoxLayer, SgResponseExt};

use crate::model::SgHttpPathModifier;
use crate::{def_plugin, MakeSgLayer};

/// RewriteFilter defines a filter that modifies a request during forwarding.
///
/// https://gateway-api.sigs.k8s.io/geps/gep-726/
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SgFilterRewriteConfig {
    /// Hostname is the value to be used to replace the Host header value during forwarding.
    pub hostname: Option<String>,
    /// Path defines parameters used to modify the path of the incoming request. The modified path is then used to construct the Location header. When empty, the request path is used as-is.
    pub path: Option<SgHttpPathModifier>,
}

#[derive(Default, Debug, Clone)]
pub struct SgFilterRewrite {
    /// Hostname is the value to be used to replace the Host header value during forwarding.
    pub hostname: Option<HeaderValue>,
    /// Path defines parameters used to modify the path of the incoming request. The modified path is then used to construct the Location header. When empty, the request path is used as-is.
    pub path: Option<SgHttpPathModifier>,
}

impl SgFilterRewrite {
    fn on_req(&self, mut req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
        if let Some(hostname) = &self.hostname {
            tracing::debug!("[Sg.Plugin.Rewrite] rewrite host {:?}", hostname);
            req.headers_mut().insert(HOST, hostname.clone());
        }
        if let Some(ref modifier) = self.path {
            let mut uri_part = req.uri().clone().into_parts();
            if let Some(matched_path) = req.extensions().get::<MatchedSgRouter>() {
                let path_match = matched_path.0.path.as_ref();
                if let Some(ref pq) = uri_part.path_and_query {
                    if let Some(path_match) = path_match {
                        if let Some(new_path) = modifier.replace(pq.path(), path_match) {
                            tracing::debug!("[Sg.Plugin.Rewrite] rewrite path from {} to {}", pq.path(), new_path);
                            let mut new_pq = new_path;
                            if let Some(query) = pq.query() {
                                new_pq.push('?');
                                new_pq.push_str(query)
                            }
                            let new_pq = hyper::http::uri::PathAndQuery::from_maybe_shared(new_pq).map_err(Response::bad_gateway)?;
                            uri_part.path_and_query = Some(new_pq)
                        }
                    }
                }
            } else {
                tracing::warn!("missing matched route");
            }
            *req.uri_mut() = Uri::from_parts(uri_part).map_err(Response::bad_gateway)?;
        }
        Ok(req)
    }
}

impl Filter for SgFilterRewrite {
    fn filter(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
        self.on_req(req)
    }
}

pub type RedirectFilterLayer = FilterRequestLayer<SgFilterRewriteConfig>;
pub type Redirect<S> = FilterRequest<SgFilterRewriteConfig, S>;

impl MakeSgLayer for SgFilterRewriteConfig {
    fn make_layer(&self) -> Result<SgBoxLayer, spacegate_kernel::BoxError> {
        let hostname = self.hostname.as_deref().map(HeaderValue::from_str).transpose()?;
        let filter = SgFilterRewrite {
            hostname,
            path: self.path.clone(),
        };
        let layer = FilterRequestLayer::new(filter);
        Ok(SgBoxLayer::new(layer))
    }
}

def_plugin!("rewrite", RewritePlugin, SgFilterRewriteConfig);
#[cfg(feature = "schema")]
crate::schema!(RewritePlugin, SgFilterRewriteConfig);
// #[cfg(test)]

// mod tests {
//     use crate::{
//         config::{http_route_dto::SgHttpPathMatchType, plugin_filter_dto::SgHttpPathModifierType},
//         instance::{SgHttpPathMatchInst, SgHttpRouteMatchInst, SgHttpRouteRuleInst},
//         plugins::context::ChosenHttpRouteRuleInst,
//     };

//     use super::*;
//     use http::{HeaderMap, Method, StatusCode, Uri, Version};
//     use hyper::Body;
//     use tardis::tokio;

//     #[tokio::test]
//     async fn test_rewrite_filter() {
//         let filter = SgFilterRewrite {
//             hostname: Some("sg_new.idealworld.group".to_string()),
//             path: Some(SgHttpPathModifier {
//                 kind: SgHttpPathModifierType::ReplacePrefixMatch,
//                 value: "/new_iam".to_string(),
//             }),
//         };

//         let matched = SgHttpRouteMatchInst {
//             path: Some(SgHttpPathMatchInst {
//                 kind: SgHttpPathMatchType::Prefix,
//                 value: "/iam".to_string(),
//                 regular: None,
//             }),
//             ..Default::default()
//         };

//         let ctx = SgRoutePluginContext::new_http(
//             Method::POST,
//             Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
//             Version::HTTP_11,
//             HeaderMap::new(),
//             Body::empty(),
//             "127.0.0.1:8080".parse().unwrap(),
//             "".to_string(),
//             Some(ChosenHttpRouteRuleInst::cloned_from(&SgHttpRouteRuleInst::default(), Some(&matched))),
//             None,
//         );

//         let (is_continue, ctx) = filter.req_filter("", ctx).await.unwrap();
//         assert!(is_continue);
//         assert_eq!(ctx.request.get_uri().to_string(), "http://sg_new.idealworld.group/new_iam/ct/001?name=sg");
//         assert_eq!(ctx.response.get_status_code(), &StatusCode::OK);

//         let (is_continue, ctx) = filter.resp_filter("", ctx).await.unwrap();
//         assert!(is_continue);
//         assert_eq!(ctx.response.get_status_code(), &StatusCode::OK);
//     }
// }
