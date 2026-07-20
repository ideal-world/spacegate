use kube::CustomResourceExt;
use spacegate_model::ext::k8s::crd::mcp_route::McpRoute;

/// Prints the MCPRoute CRD generated from the Rust custom-resource model.
fn main() {
    let manifest = serde_yaml::to_string(&McpRoute::crd()).expect("MCPRoute CRD must serialize as YAML");
    print!("{manifest}");
}
