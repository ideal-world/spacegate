pub struct TargetRefDTO {
    pub name: String,
    pub kind: Option<String>,
    pub namespace: Option<String>,
}

pub struct BackendRefDTO {
    /// Name is the kubernetes service name OR url host.
    pub name_or_host: String,
    /// Namespace is the kubernetes namespace
    pub namespace: Option<String>,
    /// Port specifies the destination port number to use for this resource.
    pub port: Option<u16>,
    // pub protocol: Option<SgProtocol>,
    //
    // /// Filters define the filters that are applied to backend that match this hostnames.
    // pub filters: Option<Vec<SgRouteFilter>>,
}
