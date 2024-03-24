use std::{
    any::TypeId,
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock, RwLock},
};

use spacegate_kernel::{BoxError, SgBoxLayer};

use crate::{
    mount::{MountPoint, MountPointIndex},
    Plugin, PluginConfig,
};

pub struct PluginInstance {
    pub plugin: TypeId,
    pub config: PluginConfig,
    pub make: Box<dyn Fn() -> Result<SgBoxLayer, BoxError> + Sync + Send + 'static>,
    pub mount_points: HashSet<MountPointIndex>,
    // maybe support hooks in the future
    // pub hooks: PluginInstanceHooks,
}

#[derive(Debug, Clone)]
pub struct PluginInstanceSnapshot {
    pub plugin: TypeId,
    pub config: PluginConfig,
    pub mount_points: HashSet<MountPointIndex>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct PluginIncetanceId {
    pub code: Cow<'static, str>,
    pub name: Option<String>,
}

impl PluginInstance {
    pub fn new<P: Plugin, M: Fn() -> Result<SgBoxLayer, BoxError> + Sync + Send + 'static>(config: PluginConfig, make: M) -> Self {
        Self {
            plugin: TypeId::of::<P>(),
            config,
            make: Box::new(make),
            mount_points: HashSet::new(),
        }
    }
    pub fn make(&self) -> Result<SgBoxLayer, BoxError> {
        (self.make)()
    }
    pub(crate) fn mount_at<M: MountPoint>(&mut self, mount_point: &mut M, index: MountPointIndex) -> Result<(), BoxError> {
        mount_point.mount(self)?;
        self.mount_points.insert(index);
        Ok(())
    }
    pub fn snapshot(&self) -> PluginInstanceSnapshot {
        PluginInstanceSnapshot {
            plugin: self.plugin,
            config: self.config.clone(),
            mount_points: self.mount_points.clone(),
        }
    }
}
