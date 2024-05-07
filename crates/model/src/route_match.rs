use serde::{Deserialize, Serialize};
/// PathMatchType specifies the semantics of how HTTP paths should be compared.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "PascalCase")]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub enum SgHttpPathMatch {
    /// Matches the URL path exactly and with case sensitivity.
    Exact {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        replace: Option<String>,
    },
    /// Matches based on a URL path prefix split by /. Matching is case sensitive and done on a path element by element basis.
    /// A path element refers to the list of labels in the path split by the / separator. When specified, a trailing / is ignored.
    Prefix {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        replace: Option<String>,
    },
    /// Matches if the URL path matches the given regular expression with case sensitivity.
    RegExp {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        replace: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub enum SgHttpHeaderMatch {
    /// Matches the HTTP header exactly and with case sensitivity.
    Exact {
        name: String,
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        replace: Option<String>,
    },
    /// Matches if the Http header matches the given regular expression with case sensitivity.
    RegExp {
        name: String,
        re: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        replace: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub enum SgHttpQueryMatch {
    /// Matches the HTTP query parameter exactly and with case sensitivity.
    Exact { key: String, value: String },
    /// Matches if the Http query parameter matches the given regular expression with case sensitivity.
    Regular { key: String, re: String },
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(transparent)]

pub struct SgHttpMethodMatch(pub String);

/// HTTPRouteMatch defines the predicate used to match requests to a given action.
/// Multiple match types are ANDed together, i.e. the match will evaluate to true only if all conditions are satisfied.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub struct SgHttpRouteMatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Path specifies a HTTP request path matcher.
    /// If this field is not specified, a default prefix match on the “/” path is provided.
    pub path: Option<SgHttpPathMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Headers specifies HTTP request header matchers.
    /// Multiple match values are ANDed together, meaning, a request must match all the specified headers to select the route.
    pub header: Option<Vec<SgHttpHeaderMatch>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Query specifies HTTP query parameter matchers.
    /// Multiple match values are ANDed together, meaning, a request must match all the specified query parameters to select the route.
    pub query: Option<Vec<SgHttpQueryMatch>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Method specifies HTTP method matcher.
    /// When specified, this route will be matched only if the request has the specified method.
    pub method: Option<Vec<SgHttpMethodMatch>>,
}
