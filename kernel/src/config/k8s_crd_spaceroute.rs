use crate::constants;
use k8s_gateway_api::{CommonRouteSpec, Group, Hostname, HttpRoute, HttpRouteFilter, HttpRouteMatch, Kind, Namespace, ObjectName, PortNumber, RouteStatus};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, kube::CustomResource, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[kube(
    group = "spacegate.idealworld.group",
    version = "v1",
    kind = "HTTPSpaceroute",
    struct = "HttpSpaceroute",
    status = "HttpSpacerouteStatus",
    namespaced
)]
pub struct HttpSpacerouteSpec {
    /// Common route information.
    #[serde(flatten)]
    pub inner: CommonRouteSpec,

    /// Hostnames defines a set of hostname that should match against the HTTP
    /// Host header to select a HTTPRoute to process the request. This matches
    /// the RFC 1123 definition of a hostname with 2 notable exceptions:
    ///
    /// 1. IPs are not allowed.
    /// 2. A hostname may be prefixed with a wildcard label (`*.`). The wildcard
    ///    label must appear by itself as the first label.
    ///
    /// If a hostname is specified by both the Listener and HTTPRoute, there
    /// must be at least one intersecting hostname for the HTTPRoute to be
    /// attached to the Listener. For example:
    ///
    /// * A Listener with `test.example.com` as the hostname matches HTTPRoutes
    ///   that have either not specified any hostnames, or have specified at
    ///   least one of `test.example.com` or `*.example.com`.
    /// * A Listener with `*.example.com` as the hostname matches HTTPRoutes
    ///   that have either not specified any hostnames or have specified at least
    ///   one hostname that matches the Listener hostname. For example,
    ///   `test.example.com` and `*.example.com` would both match. On the other
    ///   hand, `example.com` and `test.example.net` would not match.
    ///
    /// If both the Listener and HTTPRoute have specified hostnames, any
    /// HTTPRoute hostnames that do not match the Listener hostname MUST be
    /// ignored. For example, if a Listener specified `*.example.com`, and the
    /// HTTPRoute specified `test.example.com` and `test.example.net`,
    /// `test.example.net` must not be considered for a match.
    ///
    /// If both the Listener and HTTPRoute have specified hostnames, and none
    /// match with the criteria above, then the HTTPRoute is not accepted. The
    /// implementation must raise an 'Accepted' Condition with a status of
    /// `False` in the corresponding RouteParentStatus.
    ///
    /// Support: Core
    pub hostnames: Option<Vec<Hostname>>,

    /// Rules are a list of HTTP matchers, filters and actions.
    pub rules: Option<Vec<HttpRouteRule>>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct HttpSpacerouteStatus {
    /// Common route status information.
    #[serde(flatten)]
    pub inner: RouteStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HttpRouteRule {
    /// Matches define conditions used for matching the rule against incoming
    /// HTTP requests. Each match is independent, i.e. this rule will be matched
    /// if **any** one of the matches is satisfied.
    ///
    /// For example, take the following matches configuration:
    ///
    /// ```yaml
    /// matches:
    /// - path:
    ///     value: "/foo"
    ///   headers:
    ///   - name: "version"
    ///     value: "v2"
    /// - path:
    ///     value: "/v2/foo"
    /// ```
    ///
    /// For a request to match against this rule, a request must satisfy
    /// EITHER of the two conditions:
    ///
    /// - path prefixed with `/foo` AND contains the header `version: v2`
    /// - path prefix of `/v2/foo`
    ///
    /// See the documentation for HTTPRouteMatch on how to specify multiple
    /// match conditions that should be ANDed together.
    ///
    /// If no matches are specified, the default is a prefix
    /// path match on "/", which has the effect of matching every
    /// HTTP request.
    ///
    /// Proxy or Load Balancer routing configuration generated from HTTPRoutes
    /// MUST prioritize rules based on the following criteria, continuing on
    /// ties. Precedence must be given to the the Rule with the largest number
    /// of:
    ///
    /// * Characters in a matching non-wildcard hostname.
    /// * Characters in a matching hostname.
    /// * Characters in a matching path.
    /// * Header matches.
    /// * Query param matches.
    ///
    /// If ties still exist across multiple Routes, matching precedence MUST be
    /// determined in order of the following criteria, continuing on ties:
    ///
    /// * The oldest Route based on creation timestamp.
    /// * The Route appearing first in alphabetical order by
    ///   "{namespace}/{name}".
    ///
    /// If ties still exist within the Route that has been given precedence,
    /// matching precedence MUST be granted to the first matching rule meeting
    /// the above criteria.
    ///
    /// When no rules matching a request have been successfully attached to the
    /// parent a request is coming from, a HTTP 404 status code MUST be returned.
    pub matches: Option<Vec<HttpRouteMatch>>,

    /// Filters define the filters that are applied to requests that match this
    /// rule.
    ///
    /// The effects of ordering of multiple behaviors are currently unspecified.
    /// This can change in the future based on feedback during the alpha stage.
    ///
    /// Conformance-levels at this level are defined based on the type of
    /// filter:
    ///
    /// - ALL core filters MUST be supported by all implementations.
    /// - Implementers are encouraged to support extended filters.
    /// - Implementation-specific custom filters have no API guarantees across
    ///   implementations.
    ///
    /// Specifying a core filter multiple times has unspecified or custom
    /// conformance.
    ///
    /// Support: Core
    pub filters: Option<Vec<HttpRouteFilter>>,

    /// BackendRefs defines the backend(s) where matching requests should be
    /// sent.
    ///
    /// A 500 status code MUST be returned if there are no BackendRefs or
    /// filters specified that would result in a response being sent.
    ///
    /// A BackendRef is considered invalid when it refers to:
    ///
    /// * an unknown or unsupported kind of resource
    /// * a resource that does not exist
    /// * a resource in another namespace when the reference has not been
    ///   explicitly allowed by a ReferencePolicy (or equivalent concept).
    ///
    /// When a BackendRef is invalid, 500 status codes MUST be returned for
    /// requests that would have otherwise been routed to an invalid backend. If
    /// multiple backends are specified, and some are invalid, the proportion of
    /// requests that would otherwise have been routed to an invalid backend
    /// MUST receive a 500 status code.
    ///
    /// When a BackendRef refers to a Service that has no ready endpoints, it is
    /// recommended to return a 503 status code.
    ///
    /// Support: Core for Kubernetes Service
    /// Support: Custom for any other resource
    ///
    /// Support for weight: Core
    pub backend_refs: Option<Vec<HttpBackendRef>>,

    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HttpBackendRef {
    /// BackendRef is a reference to a backend to forward matched requests to.
    ///
    /// If the referent cannot be found, this HTTPBackendRef is invalid and must
    /// be dropped from the Gateway. The controller must ensure the
    /// "ResolvedRefs" condition on the Route is set to `status: False` and not
    /// configure this backend in the underlying implementation.
    ///
    /// If there is a cross-namespace reference to an *existing* object
    /// that is not covered by a ReferencePolicy, the controller must ensure the
    /// "ResolvedRefs"  condition on the Route is set to `status: False`,
    /// with the "RefNotPermitted" reason and not configure this backend in the
    /// underlying implementation.
    ///
    /// In either error case, the Message of the `ResolvedRefs` Condition
    /// should be used to provide more detail about the problem.
    ///
    /// Support: Custom
    #[serde(flatten)]
    pub backend_ref: Option<BackendRef>,

    /// Filters defined at this level should be executed if and only if the
    /// request is being forwarded to the backend defined here.
    ///
    /// Support: Custom (For broader support of filters, use the Filters field
    /// in HTTPRouteRule.)
    pub filters: Option<Vec<HttpRouteFilter>>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackendRef {
    /// Weight specifies the proportion of requests forwarded to the referenced
    /// backend. This is computed as weight/(sum of all weights in this
    /// BackendRefs list). For non-zero values, there may be some epsilon from
    /// the exact proportion defined here depending on the precision an
    /// implementation supports. Weight is not a percentage and the sum of
    /// weights does not need to equal 100.
    ///
    /// If only one backend is specified and it has a weight greater than 0,
    /// 100% of the traffic is forwarded to that backend. If weight is set to 0,
    /// no traffic should be forwarded for this entry. If unspecified, weight
    /// defaults to 1.
    ///
    /// Support for this field varies based on the context where used.
    pub weight: Option<u16>,

    pub timeout_ms: Option<u64>,

    /// BackendObjectReference references a Kubernetes object.
    #[serde(flatten)]
    pub inner: BackendObjectReference,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct BackendObjectReference {
    /// Group is the group of the referent. For example, "networking.k8s.io".
    /// When unspecified (empty string), core API group is inferred.
    pub group: Option<Group>,

    /// Kind is kind of the referent. For example "HTTPRoute" or "Service".
    /// Defaults to "Service" when not specified.
    pub kind: Option<Kind>,

    /// Name is the name of the referent.
    pub name: ObjectName,

    /// Namespace is the namespace of the backend. When unspecified, the local
    /// namespace is inferred.
    ///
    /// Note that when a namespace is specified, a ReferencePolicy object
    /// is required in the referent namespace to allow that namespace's
    /// owner to accept the reference. See the ReferencePolicy documentation
    /// for details.
    ///
    /// Support: Core
    pub namespace: Option<Namespace>,

    /// Port specifies the destination port number to use for this resource.
    /// Port is required when the referent is a Kubernetes Service. For other
    /// resources, destination port might be derived from the referent resource
    /// or this field.
    pub port: Option<PortNumber>,
}

impl From<HttpRoute> for HttpSpaceroute {
    fn from(http_route_obj: HttpRoute) -> Self {
        HttpSpaceroute {
            metadata: ObjectMeta {
                annotations: Some(if let Some(mut ann) = http_route_obj.metadata.annotations {
                    ann.insert(constants::RAW_HTTP_ROUTE_KIND.to_string(), constants::RAW_HTTP_ROUTE_KIND_DEFAULT.to_string());
                    ann
                } else {
                    BTreeMap::from([(constants::RAW_HTTP_ROUTE_KIND.to_string(), constants::RAW_HTTP_ROUTE_KIND_DEFAULT.to_string())])
                }),
                ..http_route_obj.metadata
            },
            spec: HttpSpacerouteSpec {
                inner: http_route_obj.spec.inner,
                hostnames: http_route_obj.spec.hostnames,
                rules: http_route_obj.spec.rules.map(|rules| {
                    rules
                        .into_iter()
                        .map(|rule| HttpRouteRule {
                            matches: rule.matches,
                            filters: rule.filters,
                            backend_refs: rule.backend_refs.map(|backend_refs| {
                                backend_refs
                                    .into_iter()
                                    .map(|http_backend_ref| HttpBackendRef {
                                        backend_ref: http_backend_ref.backend_ref.map(|backend_ref| BackendRef {
                                            weight: backend_ref.weight,
                                            timeout_ms: None,
                                            inner: BackendObjectReference {
                                                group: backend_ref.inner.group,
                                                kind: backend_ref.inner.kind,
                                                name: backend_ref.inner.name,
                                                namespace: backend_ref.inner.namespace,
                                                port: backend_ref.inner.port,
                                            },
                                        }),
                                        filters: http_backend_ref.filters,
                                    })
                                    .collect()
                            }),
                            timeout_ms: None,
                        })
                        .collect()
                }),
            },
            status: http_route_obj.status.map(|status| HttpSpacerouteStatus { inner: status.inner }),
        }
    }
}
