#![cfg(feature = "ext-k8s")]

use kube::CustomResourceExt;
use spacegate_model::ext::k8s::crd::mcp_route::McpRoute;

/// Removes generated prose while preserving every structural validation field.
fn remove_descriptions(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("description");
            for child in map.values_mut() {
                remove_descriptions(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                remove_descriptions(child);
            }
        }
        _ => {}
    }
}

/// Ensures the checked-in MCPRoute CRD stays structurally synchronized with the Rust model.
#[test]
fn mcp_route_manifest_matches_generated_crd() {
    let manifest = include_str!("../../../resource/kube-manifests/spacegate-mcproute.yaml");
    let mut checked_in: serde_json::Value = serde_yaml::from_str(manifest).expect("MCPRoute CRD manifest must be valid YAML");
    let mut generated = serde_json::to_value(McpRoute::crd()).expect("generated MCPRoute CRD must serialize");
    remove_descriptions(&mut checked_in);
    remove_descriptions(&mut generated);

    assert_eq!(checked_in, generated);
}

/// Ensures backend filter objects remain intact when Kubernetes stores MCPRoute.
#[test]
fn mcp_route_backend_filters_preserve_runtime_configuration() {
    let generated = serde_json::to_value(McpRoute::crd()).expect("generated MCPRoute CRD must serialize");
    let filter_items = generated
        .pointer("/spec/versions/0/schema/openAPIV3Schema/properties/spec/properties/backend_refs/items/properties/filters/items")
        .expect("MCPRoute backend filter item schema must exist");

    assert_eq!(filter_items.get("type").and_then(serde_json::Value::as_str), Some("object"));
    assert_eq!(filter_items.get("x-kubernetes-preserve-unknown-fields").and_then(serde_json::Value::as_bool), Some(true));
}
