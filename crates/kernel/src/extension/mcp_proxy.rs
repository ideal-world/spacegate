#[derive(Debug, Clone)]
pub struct McpProxyMeta {
    pub transport: String,
    pub route_type: &'static str,
    pub session_id_present: bool,
}
