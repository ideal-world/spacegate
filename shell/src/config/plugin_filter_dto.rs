use spacegate_config::model::SgRouteFilter;
use spacegate_kernel::{
    layers::{
        gateway::builder::SgGatewayLayerBuilder,
        http_route::builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
    },
    BoxError, SgBoxLayer,
};
use spacegate_plugin::{mount::{MountPoint, MountPointIndex}, MakeSgLayer, PluginConfig, SgPluginRepository};

pub fn convert_filter(filter: SgRouteFilter) -> PluginConfig {
    PluginConfig {
        code: filter.code,
        spec: filter.spec,
        name: filter.name,
    }
}

pub fn batch_convert_filter(filters: Vec<SgRouteFilter>) -> Vec<PluginConfig> {
    filters.into_iter().map(convert_filter).collect()
}

pub fn global_batch_mount_plugin<MP: MountPoint>(filters: Vec<SgRouteFilter>, mount_point: &mut MP, mount_index: MountPointIndex) {
    batch_mount_plugin(SgPluginRepository::global(), filters, mount_point, mount_index);
}

pub fn batch_mount_plugin<MP: MountPoint>(repo: &SgPluginRepository, filters: Vec<SgRouteFilter>, mount_point: &mut MP, mount_index: MountPointIndex) {
    for filter in filters {
        let config = convert_filter(filter);
        if let Err(e) = repo.mount(mount_point, mount_index.clone(), config) {
            tracing::error!("fail to mount plugin {e}")
        }
    }
}
