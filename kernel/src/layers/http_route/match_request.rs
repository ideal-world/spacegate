use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{utils::query_kv::QueryKvIter, Request, SgBody};

/// PathMatchType specifies the semantics of how HTTP paths should be compared.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value", rename_all = "PascalCase")]
pub enum SgHttpPathMatch {
    /// Matches the URL path exactly and with case sensitivity.
    Exact(String),
    /// Matches based on a URL path prefix split by /. Matching is case sensitive and done on a path element by element basis.
    /// A path element refers to the list of labels in the path split by the / separator. When specified, a trailing / is ignored.
    Prefix(String),
    /// Matches if the URL path matches the given regular expression with case sensitivity.
    #[serde(with = "serde_regex")]
    Regular(Regex),
}

impl MatchRequest for SgHttpPathMatch {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        match self {
            SgHttpPathMatch::Exact(path) => req.uri().path() == path,
            SgHttpPathMatch::Prefix(path) => {
                let mut path_segments = req.uri().path().split('/').filter(|s| !s.is_empty());
                let mut prefix_segments = path.split('/').filter(|s| !s.is_empty());
                loop {
                    match (path_segments.next(), prefix_segments.next()) {
                        (Some(path_seg), Some(prefix_seg)) => {
                            if !path_seg.eq_ignore_ascii_case(prefix_seg) {
                                return false;
                            }
                        }
                        (_, None) => return true,
                        (None, Some(_)) => return false,
                    }
                }
            }
            SgHttpPathMatch::Regular(path) => path.is_match(req.uri().path()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SgHttpHeaderMatchPolicy {
    /// Matches the HTTP header exactly and with case sensitivity.
    Exact(String),
    /// Matches if the Http header matches the given regular expression with case sensitivity.
    #[serde(with = "serde_regex")]
    Regular(Regex),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpHeaderMatch {
    /// Name is the name of the HTTP Header to be matched. Name matching MUST be case insensitive. (See https://tools.ietf.org/html/rfc7230#section-3.2).
    pub name: String,
    #[serde(flatten)]
    pub policy: SgHttpHeaderMatchPolicy,
}

impl MatchRequest for SgHttpHeaderMatch {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        match &self.policy {
            SgHttpHeaderMatchPolicy::Exact(header) => req.headers().get(&self.name).is_some_and(|v| v == header),
            SgHttpHeaderMatchPolicy::Regular(header) => req.headers().iter().any(|(k, v)| k.as_str() == self.name && v.to_str().map_or(false, |v| header.is_match(v))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SgHttpQueryMatchPolicy {
    /// Matches the HTTP query parameter exactly and with case sensitivity.
    Exact(String),
    /// Matches if the Http query parameter matches the given regular expression with case sensitivity.
    #[serde(with = "serde_regex")]
    Regular(Regex),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpQueryMatch {
    pub name: String,
    #[serde(flatten)]
    pub policy: SgHttpQueryMatchPolicy,
}

impl MatchRequest for SgHttpQueryMatch {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        let query = req.uri().query();
        if let Some(query) = query {
            let mut iter = QueryKvIter::new(query);
            match &self.policy {
                SgHttpQueryMatchPolicy::Exact(query) => iter.any(|(k, v)| k == self.name && v == Some(query)),
                SgHttpQueryMatchPolicy::Regular(query) => iter.any(|(k, v)| k == self.name && v.map_or(false, |v| query.is_match(v))),
            }
        } else {
            false
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]

pub struct SgHttpMethodMatch(pub String);

impl MatchRequest for SgHttpMethodMatch {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        req.method().as_str().eq_ignore_ascii_case(&self.0)
    }
}

/// HTTPRouteMatch defines the predicate used to match requests to a given action.
/// Multiple match types are ANDed together, i.e. the match will evaluate to true only if all conditions are satisfied.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SgHttpRouteMatch {
    /// Path specifies a HTTP request path matcher.
    /// If this field is not specified, a default prefix match on the “/” path is provided.
    pub path: Option<SgHttpPathMatch>,
    /// Headers specifies HTTP request header matchers.
    /// Multiple match values are ANDed together, meaning, a request must match all the specified headers to select the route.
    pub header: Option<Vec<SgHttpHeaderMatch>>,
    /// Query specifies HTTP query parameter matchers.
    /// Multiple match values are ANDed together, meaning, a request must match all the specified query parameters to select the route.
    pub query: Option<Vec<SgHttpQueryMatch>>,
    /// Method specifies HTTP method matcher.
    /// When specified, this route will be matched only if the request has the specified method.
    pub method: Option<Vec<SgHttpMethodMatch>>,
}

pub trait MatchRequest {
    fn match_request(&self, req: &Request<SgBody>) -> bool;
}

impl MatchRequest for SgHttpRouteMatch {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        self.path.match_request(req) && self.header.match_request(req) && self.query.match_request(req) && self.method.match_request(req)
    }
}

impl<T> MatchRequest for Option<T>
where
    T: MatchRequest,
{
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        self.as_ref().map(|r| MatchRequest::match_request(r, req)).unwrap_or(true)
    }
}

impl<T> MatchRequest for Vec<T>
where
    T: MatchRequest,
{
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        self.iter().any(|query| query.match_request(req))
    }
}
