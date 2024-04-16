use hyper::{
    http::{HeaderName, HeaderValue},
    Uri,
};
use regex::Regex;

use crate::{utils::query_kv::QueryKvIter, BoxError, Request, SgBody};

/// PathMatchType specifies the semantics of how HTTP paths should be compared.
#[derive(Debug, Clone)]
pub enum HttpPathMatchRewrite {
    /// Matches the URL path exactly and with case sensitivity.
    Exact(String, Option<String>),
    /// Matches based on a URL path prefix split by /. Matching is case sensitive and done on a path element by element basis.
    /// A path element refers to the list of labels in the path split by the / separator. When specified, a trailing / is ignored.
    Prefix(String, Option<String>),
    /// Matches if the URL path matches the given regular expression with case sensitivity.
    RegExp(Regex, Option<String>),
}

impl HttpPathMatchRewrite {
    pub fn prefix<S: Into<String>>(s: S) -> Self {
        Self::Prefix(s.into(), None)
    }
    pub fn exact<S: Into<String>>(s: S) -> Self {
        Self::Exact(s.into(), None)
    }
    pub fn regex(re: Regex) -> Self {
        Self::RegExp(re, None)
    }
    pub fn replace_with(self, replace: impl Into<String>) -> Self {
        match self {
            HttpPathMatchRewrite::Exact(path, _) => HttpPathMatchRewrite::Exact(path, Some(replace.into())),
            HttpPathMatchRewrite::Prefix(path, _) => HttpPathMatchRewrite::Prefix(path, Some(replace.into())),
            HttpPathMatchRewrite::RegExp(re, _) => HttpPathMatchRewrite::RegExp(re, Some(replace.into())),
        }
    }
    pub fn rewrite(&self, path: &str) -> Option<String> {
        match self {
            HttpPathMatchRewrite::Exact(_, Some(replace)) => {
                if replace.eq_ignore_ascii_case(path) {
                    Some(replace.clone())
                } else {
                    None
                }
            }
            HttpPathMatchRewrite::Prefix(prefix, Some(replace)) => {
                fn not_empty(s: &&str) -> bool {
                    !s.is_empty()
                }
                let mut path_segments = path.split('/').filter(not_empty);
                let mut prefix_segments = prefix.split('/').filter(not_empty);
                loop {
                    match (path_segments.next(), prefix_segments.next()) {
                        (Some(path_seg), Some(prefix_seg)) => {
                            if !path_seg.eq_ignore_ascii_case(prefix_seg) {
                                return None;
                            }
                        }
                        (None, None) => {
                            // handle with duplicated stash and no stash
                            let mut new_path = String::from("/");
                            new_path.push_str(replace.trim_start_matches('/'));
                            return Some(new_path);
                        }
                        (Some(rest_path), None) => {
                            let mut new_path = String::from("/");
                            let replace_value = replace.trim_matches('/');
                            new_path.push_str(replace_value);
                            if !replace_value.is_empty() {
                                new_path.push('/');
                            }
                            new_path.push_str(rest_path);
                            for seg in path_segments {
                                new_path.push('/');
                                new_path.push_str(seg);
                            }
                            if path.ends_with('/') {
                                new_path.push('/')
                            }
                            return Some(new_path);
                        }
                        (None, Some(_)) => return None,
                    }
                }
            }
            HttpPathMatchRewrite::RegExp(re, Some(replace)) => Some(re.replace(path, replace).to_string()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SgHttpHeaderMatchRewritePolicy {
    /// Matches the HTTP header exactly and with case sensitivity.
    Exact(HeaderValue, Option<HeaderValue>),
    /// Matches if the Http header matches the given regular expression with case sensitivity.
    Regular(Regex, Option<String>),
}

#[derive(Debug, Clone)]
pub struct SgHttpHeaderMatchRewrite {
    /// Name is the name of the HTTP Header to be matched. Name matching MUST be case insensitive. (See https://tools.ietf.org/html/rfc7230#section-3.2).
    pub header_name: HeaderName,
    pub policy: SgHttpHeaderMatchRewritePolicy,
}

impl SgHttpHeaderMatchRewrite {
    pub fn regex(name: impl Into<HeaderName>, re: Regex) -> Self {
        Self {
            header_name: name.into(),
            policy: SgHttpHeaderMatchRewritePolicy::Regular(re, None),
        }
    }
    pub fn exact(name: impl Into<HeaderName>, value: impl Into<HeaderValue>) -> Self {
        Self {
            header_name: name.into(),
            policy: SgHttpHeaderMatchRewritePolicy::Exact(value.into(), None),
        }
    }
    pub fn rewrite(&self, req: &Request<SgBody>) -> Option<HeaderValue> {
        let header_value = req.headers().get(&self.header_name)?;
        let s = header_value.to_str().ok()?;
        match &self.policy {
            SgHttpHeaderMatchRewritePolicy::Exact(_, Some(replace)) => {
                if s == replace {
                    Some(replace.clone())
                } else {
                    None
                }
            }
            SgHttpHeaderMatchRewritePolicy::Regular(re, Some(replace)) => {
                if re.is_match(s) {
                    Some(HeaderValue::from_str(replace).ok()?)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SgHttpQueryMatchPolicy {
    /// Matches the HTTP query parameter exactly and with case sensitivity.
    Exact(String),
    /// Matches if the Http query parameter matches the given regular expression with case sensitivity.
    Regular(Regex),
}

#[derive(Debug, Clone)]
pub struct HttpQueryMatch {
    pub name: String,
    pub policy: SgHttpQueryMatchPolicy,
}

#[derive(Default, Debug, Clone)]

pub struct HttpMethodMatch(pub String);

/// HTTPRouteMatch defines the predicate used to match requests to a given action.
/// Multiple match types are ANDed together, i.e. the match will evaluate to true only if all conditions are satisfied.
#[derive(Default, Debug, Clone)]
pub struct HttpRouteMatch {
    /// Path specifies a HTTP request path matcher.
    /// If this field is not specified, a default prefix match on the “/” path is provided.
    pub path: Option<HttpPathMatchRewrite>,
    /// Headers specifies HTTP request header matchers.
    /// Multiple match values are ANDed together, meaning, a request must match all the specified headers to select the route.
    pub header: Option<Vec<SgHttpHeaderMatchRewrite>>,
    /// Query specifies HTTP query parameter matchers.
    /// Multiple match values are ANDed together, meaning, a request must match all the specified query parameters to select the route.
    pub query: Option<Vec<HttpQueryMatch>>,
    /// Method specifies HTTP method matcher.
    /// When specified, this route will be matched only if the request has the specified method.
    pub method: Option<Vec<HttpMethodMatch>>,
}

impl HttpRouteMatch {
    /// rewrite request path and headers
    /// # Errors
    /// Rewritten path is invalid.
    pub fn rewrite(&self, req: &mut Request<SgBody>) -> Result<(), BoxError> {
        if let Some(headers_match) = self.header.as_ref() {
            for header_match in headers_match {
                if let (Some(replace), Some(v)) = (header_match.rewrite(req), req.headers_mut().get_mut(&header_match.header_name)) {
                    *v = replace;
                }
            }
        }
        let path_match = self.path.as_ref();
        if let (Some(pq), Some(path_match)) = (req.uri().path_and_query(), path_match) {
            let old_path = pq.path();
            if let Some(new_path) = path_match.rewrite(old_path) {
                let mut uri_part = req.uri().clone().into_parts();
                tracing::debug!("[Sg.Rewrite] rewrite path from {} to {}", old_path, new_path);
                let mut new_pq = new_path;
                if let Some(query) = pq.query() {
                    new_pq.push('?');
                    new_pq.push_str(query)
                }
                let new_pq = hyper::http::uri::PathAndQuery::from_maybe_shared(new_pq)?;
                uri_part.path_and_query = Some(new_pq);
                *req.uri_mut() = Uri::from_parts(uri_part)?;
            }
        }
        Ok(())
    }
}

pub trait MatchRequest {
    fn match_request(&self, req: &Request<SgBody>) -> bool;
}

impl MatchRequest for HttpQueryMatch {
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

impl From<HttpPathMatchRewrite> for HttpRouteMatch {
    fn from(val: HttpPathMatchRewrite) -> Self {
        HttpRouteMatch {
            path: Some(val),
            header: None,
            query: None,
            method: None,
        }
    }
}

impl From<SgHttpHeaderMatchRewrite> for HttpRouteMatch {
    fn from(value: SgHttpHeaderMatchRewrite) -> Self {
        HttpRouteMatch {
            path: None,
            header: Some(vec![value]),
            query: None,
            method: None,
        }
    }
}

impl From<HttpQueryMatch> for HttpRouteMatch {
    fn from(value: HttpQueryMatch) -> Self {
        HttpRouteMatch {
            path: None,
            header: None,
            query: Some(vec![value]),
            method: None,
        }
    }
}

impl From<HttpMethodMatch> for HttpRouteMatch {
    fn from(value: HttpMethodMatch) -> Self {
        HttpRouteMatch {
            path: None,
            header: None,
            query: None,
            method: Some(vec![value]),
        }
    }
}

impl MatchRequest for HttpPathMatchRewrite {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        match self {
            HttpPathMatchRewrite::Exact(path, _) => req.uri().path() == path,
            HttpPathMatchRewrite::Prefix(path, _) => {
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
            HttpPathMatchRewrite::RegExp(path, _) => path.is_match(req.uri().path()),
        }
    }
}

impl MatchRequest for SgHttpHeaderMatchRewrite {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        match &self.policy {
            SgHttpHeaderMatchRewritePolicy::Exact(header, _) => req.headers().get(&self.header_name).is_some_and(|v| v == header),
            SgHttpHeaderMatchRewritePolicy::Regular(header, _) => {
                req.headers().iter().any(|(k, v)| k.as_str() == self.header_name && v.to_str().map_or(false, |v| header.is_match(v)))
            }
        }
    }
}

impl MatchRequest for HttpMethodMatch {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        req.method().as_str().eq_ignore_ascii_case(&self.0)
    }
}

impl MatchRequest for HttpRouteMatch {
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

#[test]
fn test_match_path() {
    let req = Request::builder().uri("https://localhost:8080/child/subApp").body(SgBody::empty()).expect("invalid request");
    assert!(HttpPathMatchRewrite::Prefix("/child/subApp".into(), None).match_request(&req));
}
