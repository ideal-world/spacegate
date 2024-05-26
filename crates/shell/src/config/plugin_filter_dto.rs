use spacegate_config::{model::PluginConfig, PluginInstanceId};
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

pub fn global_batch_mount_plugin<MP: MountPoint>(plugins: Vec<PluginInstanceId>, mount_point: &mut MP, mount_index: MountPointIndex) {
    batch_mount_plugin(PluginRepository::global(), plugins, mount_point, mount_index);
}

pub fn batch_mount_plugin<MP: MountPoint>(repo: &PluginRepository, plugins: Vec<PluginInstanceId>, mount_point: &mut MP, mount_index: MountPointIndex) {
    for plugin in plugins {
        if let Err(e) = repo.mount(mount_point, mount_index.clone(), plugin) {
            tracing::error!("fail to mount plugin {e}")
        }
    }
}
