use spacegate_config::{model::PluginConfig, PluginBinding};
use spacegate_plugin::{
    mount::{MountPoint, MountPointIndex},
    PluginRepository,
};

pub fn global_batch_update_plugin(plugins: Vec<PluginConfig>) {
    for plugin in plugins {
        match PluginRepository::global().create_or_update_instance(plugin) {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("fail to create or update plugin {e}")
            }
        }
    }
}

pub fn global_batch_mount_plugin<MP: MountPoint>(plugins: Vec<PluginBinding>, mount_point: &mut MP, mount_index: MountPointIndex) {
    batch_mount_plugin(PluginRepository::global(), plugins, mount_point, mount_index);
}

pub fn batch_mount_plugin<MP: MountPoint>(repo: &PluginRepository, plugins: Vec<PluginBinding>, mount_point: &mut MP, mount_index: MountPointIndex) {
    for binding in order_plugin_bindings(plugins) {
        if let Err(e) = repo.mount(mount_point, mount_index.clone(), binding.id) {
            tracing::error!("fail to mount plugin {e}")
        }
    }
}

pub fn order_plugin_bindings(mut bindings: Vec<PluginBinding>) -> Vec<PluginBinding> {
    bindings.sort_by(|left, right| right.priority.cmp(&left.priority));
    bindings
}

#[cfg(test)]
mod tests {
    use spacegate_config::{PluginBinding, PluginInstanceName};

    use super::order_plugin_bindings;

    #[test]
    fn orders_bindings_by_descending_priority_and_preserves_ties() {
        let bindings = vec![
            PluginBinding::new("native", PluginInstanceName::named("first"), 10),
            PluginBinding::new("wasm", PluginInstanceName::named("second"), 100),
            PluginBinding::new("native", PluginInstanceName::named("third"), 10),
        ];

        let ordered = order_plugin_bindings(bindings);
        let names = ordered.into_iter().map(|binding| binding.id.name.to_raw_str()).collect::<Vec<_>>();

        assert_eq!(names, ["second", "first", "third"]);
    }
}
