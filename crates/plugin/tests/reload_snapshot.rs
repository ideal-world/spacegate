use spacegate_kernel::{helper_layers::function::Inner, BoxError, SgBody};
use spacegate_model::{PluginConfig, PluginInstanceId, PluginInstanceName};
use spacegate_plugin::{Plugin, PluginRepository};

struct SnapshotPlugin;
impl Plugin for SnapshotPlugin {
    const CODE: &'static str = "reload-snapshot-test";
    fn create(config: PluginConfig) -> Result<Self, BoxError> {
        if config.spec.get("reject").and_then(serde_json::Value::as_bool) == Some(true) {
            return Err("invalid test config".into());
        }
        Ok(Self)
    }
    async fn call(&self, req: hyper::Request<SgBody>, inner: Inner) -> Result<hyper::Response<SgBody>, BoxError> {
        Ok(inner.call(req).await)
    }
}

#[test]
fn reload_snapshot_tracks_successful_config_and_preserves_it_on_failed_update() {
    let repo = PluginRepository::new();
    repo.register::<SnapshotPlugin>();
    let id = PluginInstanceId::new(SnapshotPlugin::CODE, PluginInstanceName::named("quota"));
    let config = |size, reject| PluginConfig {
        id: id.clone(),
        display_name: Some(format!("pool-{size}")),
        spec: serde_json::json!({"size":size,"reject":reject}),
    };
    repo.create_or_update_instance(config(32, false)).unwrap();
    repo.create_or_update_instance(config(256, false)).unwrap();
    let snapshot = repo.instance_snapshot(id.clone()).unwrap();
    assert_eq!(snapshot.config.spec["size"], 256, "management and recovery must observe the loaded config");
    assert_eq!(snapshot.config.display_name.as_deref(), Some("pool-256"));
    assert!(repo.create_or_update_instance(config(512, true)).is_err());
    assert_eq!(repo.instance_snapshot(id).unwrap().config.spec["size"], 256);
}

#[test]
fn gateway_service_keeps_plugin_mount_alive_until_service_is_dropped() {
    use spacegate_kernel::service::http_gateway::Gateway;
    use spacegate_plugin::mount::MountPointIndex;
    let repo = PluginRepository::new();
    repo.register::<SnapshotPlugin>();
    let id = PluginInstanceId::new(SnapshotPlugin::CODE, PluginInstanceName::named("mounted"));
    repo.create_or_update_instance(PluginConfig { id:id.clone(), display_name:None, spec:serde_json::json!({}) }).unwrap();
    let service = {
        let mut gateway = Gateway::builder("mounted-gateway").build();
        repo.mount(&mut gateway, MountPointIndex::Gateway {gateway:"mounted-gateway".into()}, id.clone()).unwrap();
        gateway.as_service()
    };
    assert_eq!(repo.instance_snapshot(id.clone()).unwrap().mount_points.len(),1,"running Gateway service must retain the plugin mount after its builder is dropped");
    drop(service);
    assert!(repo.instance_snapshot(id).unwrap().mount_points.is_empty(),"dropped services must release their mount markers");
}
