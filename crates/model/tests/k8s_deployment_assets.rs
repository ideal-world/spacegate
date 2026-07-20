#![cfg(feature = "ext-k8s")]

use serde::Deserialize;
use std::path::Path;

/// Parses every YAML document from a multi-resource Kubernetes manifest.
fn yaml_documents(manifest: &str) -> Vec<serde_json::Value> {
    serde_yaml::Deserializer::from_str(manifest).map(|document| serde_json::Value::deserialize(document).expect("Kubernetes manifest document must be valid YAML")).collect()
}

/// Checks whether the named RBAC object grants access to a resource.
fn rbac_contains_resource(manifest: &str, kind: &str, name: &str, resource: &str) -> bool {
    yaml_documents(manifest).into_iter().any(|document| {
        document.get("kind").and_then(serde_json::Value::as_str) == Some(kind)
            && document.pointer("/metadata/name").and_then(serde_json::Value::as_str) == Some(name)
            && document
                .get("rules")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|rule| rule.get("resources").and_then(serde_json::Value::as_array))
                .flatten()
                .any(|value| value.as_str() == Some(resource))
    })
}

/// Ensures every K8s component that watches or manages routes can access MCPRoute.
#[test]
fn rbac_manifests_include_mcp_route() {
    let gateway = include_str!("../../../resource/kube-manifests/spacegate-gateway.yaml");
    let admin = include_str!("../../../resource/kube-manifests/spacegate-admin-server.yaml");
    let cluster = include_str!("../../../deploy/k8s/ai-gateway/spacegate-rbac-cluster.yaml");

    assert!(rbac_contains_resource(gateway, "Role", "spacegate", "mcproutes"));
    assert!(rbac_contains_resource(admin, "ClusterRole", "spacegate-admin", "mcproutes"));
    assert!(rbac_contains_resource(cluster, "ClusterRole", "spacegate-k8s-config", "mcproutes"));
}

/// Ensures the Spacegate controller can update the cluster-scoped GatewayClass status it owns.
#[test]
fn rbac_manifests_allow_gateway_class_status_reconciliation() {
    let gateway = include_str!("../../../resource/kube-manifests/spacegate-gateway.yaml");

    assert!(rbac_contains_resource(gateway, "ClusterRole", "spacegate-gatewayclass-status", "gatewayclasses"));
    assert!(rbac_contains_resource(gateway, "ClusterRole", "spacegate-gatewayclass-status", "gatewayclasses/status"));
}

/// Ensures CRDs queried during K8s startup are installed before the controller.
#[test]
fn test_deploy_applies_required_custom_resource_definitions() {
    let deploy_script = include_str!("../../../deploy/k8s/test-spacegate/deploy.sh");

    assert!(deploy_script.contains("resource/kube-manifests/spacegate-httproute.yaml"));
    assert!(deploy_script.contains("resource/kube-manifests/spacegate-mcproute.yaml"));
    assert!(deploy_script.contains("resource/kube-manifests/higress-wasmplugin-crd.yaml"));
}

/// Ensures production deployment entry points install every CRD queried by the K8s backend.
#[test]
fn production_deployment_docs_install_route_crds_before_spacegate() {
    let production = include_str!("../../../docs/k8s/production-deployment.md");
    let deploy_readme = include_str!("../../../deploy/README.md");

    for document in [production, deploy_readme] {
        assert!(document.contains("resource/kube-manifests/spacegate-httproute.yaml"));
        assert!(document.contains("resource/kube-manifests/spacegate-mcproute.yaml"));
    }
}

/// Ensures published K8s images contain the Wasm and dylib runtimes expected by the manifests.
#[test]
fn release_workflow_builds_wasm_and_dylib_capable_gateway() {
    let workflow = include_str!("../../../.github/workflows/cicd.yml");
    let expected = "cargo build --release -p spacegate --features build-k8s,wasm,dylib";

    assert_eq!(workflow.matches(expected).count(), 2);
}

/// Ensures the base DaemonSet keeps the image-bundled plugin directory and exposes a mount directory.
#[test]
fn gateway_manifest_declares_native_dylib_plugin_directories() {
    let gateway = include_str!("../../../resource/kube-manifests/spacegate-gateway.yaml");
    let daemon_set = yaml_documents(gateway)
        .into_iter()
        .find(|document| document.get("kind").and_then(serde_json::Value::as_str) == Some("DaemonSet"))
        .expect("spacegate gateway manifest must include a DaemonSet");
    let env = daemon_set.pointer("/spec/template/spec/containers/0/env").and_then(serde_json::Value::as_array).expect("spacegate container must declare environment variables");
    let plugins = env
        .iter()
        .find(|entry| entry.get("name").and_then(serde_json::Value::as_str) == Some("PLUGINS"))
        .and_then(|entry| entry.get("value"))
        .and_then(serde_json::Value::as_str)
        .expect("spacegate container must declare PLUGINS");

    assert!(plugins.contains("/lib/spacegate/plugins"));
    assert!(plugins.contains("/var/lib/spacegate/plugins"));

    let volume_mounts = daemon_set
        .pointer("/spec/template/spec/containers/0/volumeMounts")
        .and_then(serde_json::Value::as_array)
        .expect("spacegate container must mount the external plugin directory");
    assert!(volume_mounts.iter().any(|mount| {
        mount.get("name").and_then(serde_json::Value::as_str) == Some("external-plugins")
            && mount.get("mountPath").and_then(serde_json::Value::as_str) == Some("/var/lib/spacegate/plugins")
    }));

    let volumes = daemon_set.pointer("/spec/template/spec/volumes").and_then(serde_json::Value::as_array).expect("spacegate pod must define the external plugin volume");
    assert!(volumes.iter().any(|volume| { volume.get("name").and_then(serde_json::Value::as_str) == Some("external-plugins") && volume.get("emptyDir").is_some() }));
}

/// Keeps non-runnable Wasm examples out of the directory used for base manifests.
#[test]
fn base_manifest_directory_excludes_wasm_hello_example() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(!root.join("resource/kube-manifests/wasmplugin-hello-example.yaml").exists());
    assert!(root.join("resource/kube-manifests/examples/wasmplugin-hello-example.yaml.example").exists());
}

/// Ensures the base DaemonSet exposes every process-level observability setting.
#[test]
fn gateway_manifest_declares_observability_environment() {
    let gateway = include_str!("../../../resource/kube-manifests/spacegate-gateway.yaml");
    let daemon_set = yaml_documents(gateway)
        .into_iter()
        .find(|document| document.get("kind").and_then(serde_json::Value::as_str) == Some("DaemonSet"))
        .expect("spacegate gateway manifest must include a DaemonSet");
    let env = daemon_set.pointer("/spec/template/spec/containers/0/env").and_then(serde_json::Value::as_array).expect("spacegate container must declare environment variables");
    let names = env.iter().filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str)).collect::<Vec<_>>();

    for required in [
        "RUST_LOG",
        "SPACEGATE_OTEL_ENABLED",
        "SPACEGATE_OTEL_SERVICE_NAME",
        "SPACEGATE_OTEL_ENDPOINT",
        "SPACEGATE_OTEL_PROTOCOL",
        "SPACEGATE_OTEL_TRACES_ENABLED",
        "SPACEGATE_OTEL_TRACES_SAMPLE_RATIO",
        "SPACEGATE_OTEL_METRICS_ENABLED",
        "SPACEGATE_OTEL_METRICS_EXPORT_INTERVAL_MS",
        "SPACEGATE_OTEL_LOGS_ENABLED",
        "SPACEGATE_OTEL_LOGS_LEVEL",
    ] {
        assert!(names.contains(&required), "missing environment variable {required}");
    }
}

/// Keeps the operations-facing AI Gateway bundle separate from demo resources.
#[test]
fn ai_gateway_production_bundle_uses_external_redis_and_file_wasm() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let production = root.join("deploy/k8s/ai-gateway/production");

    assert!(production.join("kustomization.yaml").is_file(), "missing production Kustomize entry point");
    assert!(production.join("README.md").is_file(), "missing operations deployment guide");

    let kustomization = std::fs::read_to_string(production.join("kustomization.yaml")).expect("read production Kustomization");
    assert!(kustomization.contains("ai-gateway-service.yaml"));
    assert!(kustomization.contains("httproute-ai.yaml"));
    assert!(kustomization.contains("sgfilter-ai-gateway-queue.yaml"));
    assert!(!kustomization.contains("gateway-ai.yaml"));
    assert!(!kustomization.contains("mock-upstream.yaml"));
    assert!(!kustomization.contains("redis.yaml"));
    assert!(
        root.join("deploy/k8s/ai-gateway/production-with-dedicated-gateway/kustomization.yaml").is_file(),
        "missing optional dedicated Gateway overlay"
    );

    let service = std::fs::read_to_string(production.join("ai-gateway-service.yaml")).expect("read AI Gateway service manifest");
    assert!(service.contains("secretKeyRef"));
    assert!(service.contains("name: ai-gateway-queue"));
    assert!(service.contains("ai-gateway-queue-redis"));

    let filter = std::fs::read_to_string(production.join("sgfilter-ai-gateway-queue.yaml")).expect("read AI Gateway filter manifest");
    assert!(filter.contains("file:///lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm"));
    assert!(!filter.contains("file:///plugins/ai-gateway-queue.wasm"));
    assert!(filter.contains("http://ai-gateway-queue:18080"));

    let gateway = include_str!("../../../resource/kube-manifests/spacegate-gateway.yaml");
    assert!(gateway.contains("mountPath: /plugins"));
    assert!(gateway.contains("path: /opt/spacegate/wasm"));
}

/// Ensures the Admin deployment receives its Nginx proxy configuration from Kubernetes.
#[test]
fn admin_manifest_mounts_nginx_config_map() {
    let admin = include_str!("../../../resource/kube-manifests/spacegate-admin-server.yaml");
    let documents = yaml_documents(admin);

    assert!(documents.iter().any(|document| {
        document.get("kind").and_then(serde_json::Value::as_str) == Some("ConfigMap")
            && document.pointer("/metadata/name").and_then(serde_json::Value::as_str) == Some("spacegate-admin-nginx")
            && document.pointer("/data/nginx.conf").and_then(serde_json::Value::as_str).is_some_and(|config| config.contains("proxy_pass http://ai-gateway-queue:18080;"))
    }));

    let deployment = documents
        .iter()
        .find(|document| document.get("kind").and_then(serde_json::Value::as_str) == Some("Deployment"))
        .expect("spacegate admin manifest must include a Deployment");
    let mounts = deployment.pointer("/spec/template/spec/containers/0/volumeMounts").and_then(serde_json::Value::as_array).expect("admin container must mount Nginx config");
    assert!(mounts.iter().any(|mount| {
        mount.get("name").and_then(serde_json::Value::as_str) == Some("nginx-config")
            && mount.get("mountPath").and_then(serde_json::Value::as_str) == Some("/etc/nginx/nginx.conf")
            && mount.get("subPath").and_then(serde_json::Value::as_str) == Some("nginx.conf")
            && mount.get("readOnly").and_then(serde_json::Value::as_bool) == Some(true)
    }));

    let start_script = include_str!("../../../resource/docker/spacegate-admin/start.sh");
    assert!(start_script.contains("./admin-server -H 127.0.0.1 -p 9081 -c \"$CONFIG\""));
}
