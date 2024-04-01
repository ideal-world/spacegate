use std::{
    any::{Any, TypeId},
    borrow::Cow,
    collections::{HashMap, HashSet},
    convert::Infallible,
    fmt::Display,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use spacegate_kernel::{helper_layers::function::FnLayer, BoxError, BoxResult, SgBoxLayer};
use spacegate_model::PluginConfig;

use crate::{
    mount::{MountPoint, MountPointIndex},
};

// pub struct PluginInstanceRef {
//     id: PluginInstanceId,
// }

pub struct PluginInstance {
    // data
    pub config: PluginConfig,
    pub mount_points: HashSet<MountPointIndex>,
    pub hooks: PluginInstanceHooks,
    pub resource: PluginInstanceResource,
    pub plugin_function: crate::layer::PluginFunction,
}

pub type BoxMakeFn = Box<dyn Fn(&PluginInstance) -> Result<SgBoxLayer, BoxError> + Sync + Send + 'static>;
type PluginInstanceHook = Box<dyn Fn(&PluginInstance) -> Result<(), BoxError> + Send + Sync + 'static>;

#[derive(Default)]
pub struct PluginInstanceResource {
    inner: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl PluginInstanceResource {
    pub fn get<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.inner.get(&TypeId::of::<T>()).and_then(|v| v.downcast_ref())
    }
    pub fn get_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut T> {
        self.inner.get_mut(&TypeId::of::<T>()).and_then(|v| v.downcast_mut())
    }
    pub fn insert<T: 'static + Send + Sync>(&mut self, value: T) {
        self.inner.insert(TypeId::of::<T>(), Box::new(value));
    }
}

impl std::fmt::Debug for PluginInstanceResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(stringify!(PluginInstanceResource)).finish()
    }
}

#[derive(Default)]
pub struct PluginInstanceHooks {
    pub after_create: Option<PluginInstanceHook>,
    pub before_mount: Option<PluginInstanceHook>,
    pub after_mount: Option<PluginInstanceHook>,
    pub before_destroy: Option<PluginInstanceHook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstanceSnapshot {
    pub config: PluginConfig,
    pub mount_points: HashSet<MountPointIndex>,
}

macro_rules! expose_hooks {
    ($($hook: ident, $setter: ident)*) => {
        $(
            pub(crate) fn $hook(&self) -> BoxResult<()> {
                self.call_hook(&self.hooks.$hook)
            }
            pub fn $setter<M>(&mut self, hook: M)
            where M: Fn(&PluginInstance) -> Result<(), BoxError> + Send + Sync + 'static
            {
                self.hooks.$hook = Some(Box::new(hook))
            }
        )*
    };
}

impl PluginInstance {
    pub(crate) fn mount_at<M: MountPoint>(&mut self, mount_point: &mut M, index: MountPointIndex) -> Result<(), BoxError> {
        mount_point.mount(self)?;
        self.mount_points.insert(index);
        Ok(())
    }
    pub fn snapshot(&self) -> PluginInstanceSnapshot {
        PluginInstanceSnapshot {
            config: self.config.clone(),
            mount_points: self.mount_points.clone(),
        }
    }
    pub(crate) fn call_hook(&self, hook: &Option<PluginInstanceHook>) -> BoxResult<()> {
        if let Some(ref hook) = hook {
            (hook)(self)
        } else {
            Ok(())
        }
    }
    pub fn make(&self) -> SgBoxLayer {
        SgBoxLayer::new(FnLayer::new(self.plugin_function.clone()))
    }
    expose_hooks! {
        after_create, set_after_create
        before_mount, set_before_create
        after_mount, set_after_mount
        before_destroy, set_before_destroy
    }
}
