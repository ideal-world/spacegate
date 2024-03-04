use spacegate_config::model as config;
use spacegate_kernel::{layers::http_route::match_request as kernel, BoxError};
use tardis::regex::Regex;

/// convert [`config::SgHttpRouteMatch`] into [`kernel::SgHttpRouteMatch`]
pub(crate) fn convert_config_to_kernel(config_match: config::SgHttpRouteMatch) -> Result<kernel::SgHttpRouteMatch, BoxError> {
    Ok(kernel::SgHttpRouteMatch {
        path: match config_match.path {
            Some(config::SgHttpPathMatch::Exact(path)) => Some(kernel::SgHttpPathMatch::Exact(path)),
            Some(config::SgHttpPathMatch::Prefix(path)) => Some(kernel::SgHttpPathMatch::Prefix(path)),
            Some(config::SgHttpPathMatch::Regular(path)) => Some(kernel::SgHttpPathMatch::Regular(Regex::new(&path)?)),
            None => None,
        },
        header: match config_match.header {
            Some(headers) => Some(
                headers
                    .into_iter()
                    .map(|header| match header {
                        config::SgHttpHeaderMatch::Exact { name, value } => Ok(kernel::SgHttpHeaderMatch {
                            name,
                            policy: kernel::SgHttpHeaderMatchPolicy::Exact(value.clone()),
                        }),
                        config::SgHttpHeaderMatch::Regular { name, re } => Ok(kernel::SgHttpHeaderMatch {
                            name,
                            policy: kernel::SgHttpHeaderMatchPolicy::Regular(Regex::new(&re)?),
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
                        config::SgHttpQueryMatch::Exact { key, value } => Ok(kernel::SgHttpQueryMatch {
                            name: key,
                            policy: kernel::SgHttpQueryMatchPolicy::Exact(value.clone()),
                        }),
                        config::SgHttpQueryMatch::Regular { key, re } => Ok(kernel::SgHttpQueryMatch {
                            name: key,
                            policy: kernel::SgHttpQueryMatchPolicy::Regular(Regex::new(&re)?),
                        }),
                    })
                    .collect::<Result<Vec<_>, BoxError>>()?,
            ),
            None => None,
        },
        method: config_match.method.map(|method| method.into_iter().map(|x| kernel::SgHttpMethodMatch(x.0)).collect()),
    })
}
