use std::str::FromStr;

use hyper::header::{HeaderName, HeaderValue};
use regex::Regex;
use spacegate_config::model as config;
use spacegate_kernel::{service::http_route::match_request as kernel, BoxError};

/// convert [`config::SgHttpRouteMatch`] into [`kernel::SgHttpRouteMatch`]
pub(crate) fn convert_config_to_kernel(config_match: config::SgHttpRouteMatch) -> Result<kernel::HttpRouteMatch, BoxError> {
    Ok(kernel::HttpRouteMatch {
        path: match config_match.path {
            Some(config::SgHttpPathMatch::Exact { value, replace }) => Some(kernel::HttpPathMatchRewrite::Exact(value, replace)),
            Some(config::SgHttpPathMatch::Prefix { value, replace }) => Some(kernel::HttpPathMatchRewrite::Prefix(value, replace)),
            Some(config::SgHttpPathMatch::RegExp { value, replace }) => Some(kernel::HttpPathMatchRewrite::RegExp(Regex::new(&value)?, replace)),
            None => None,
        },
        header: match config_match.header {
            Some(headers) => Some(
                headers
                    .into_iter()
                    .map(|header| match header {
                        config::SgHttpHeaderMatch::Exact { name, value, replace } => Ok(kernel::SgHttpHeaderMatchRewrite {
                            header_name: HeaderName::from_str(&name)?,
                            policy: kernel::SgHttpHeaderMatchRewritePolicy::Exact(HeaderValue::from_str(&value)?, replace.as_deref().map(HeaderValue::from_str).transpose()?),
                        }),
                        config::SgHttpHeaderMatch::RegExp { name, re, replace } => Ok(kernel::SgHttpHeaderMatchRewrite {
                            header_name: HeaderName::from_str(&name)?,
                            policy: kernel::SgHttpHeaderMatchRewritePolicy::Regular(Regex::new(&re)?, replace),
                        }),
                    })
                    .collect::<Result<Vec<_>, BoxError>>()?,
            ),
            None => None,
        },
        query: match config_match.query {
            Some(queries) => Some(
                queries
                    .into_iter()
                    .map(|query| match query {
                        config::SgHttpQueryMatch::Exact { key, value } => Ok(kernel::HttpQueryMatch {
                            name: key,
                            policy: kernel::SgHttpQueryMatchPolicy::Exact(value.clone()),
                        }),
                        config::SgHttpQueryMatch::Regular { key, re } => Ok(kernel::HttpQueryMatch {
                            name: key,
                            policy: kernel::SgHttpQueryMatchPolicy::Regular(Regex::new(&re)?),
                        }),
                    })
                    .collect::<Result<Vec<_>, BoxError>>()?,
            ),
            None => None,
        },
        method: config_match.method.map(|method| method.into_iter().map(|x| kernel::HttpMethodMatch(x.0)).collect()),
    })
}
