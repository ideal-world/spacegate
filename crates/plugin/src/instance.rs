use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Weak,
    },
};

use serde::{Deserialize, Serialize};
use spacegate_kernel::{helper_layers::function::FnLayer, BoxError, BoxLayer, BoxResult};
use spacegate_model::PluginConfig;

use crate::mount::{MountPoint, MountPointIndex};

pub struct PluginInstance {
    pub config: PluginConfig,
    pub mount_points: HashMap<MountPointIndex, DropTracer>,
    pub hooks: PluginInstanceHooks,
    pub plugin_function: crate::layer::PluginFunction,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DropMarker {
    drop_signal: Arc<u64>,
}

#[derive(Debug, Clone, Default)]

pub(crate) struct DropMarkerSet {
    pub(crate) inner: HashSet<DropMarker>,
}

#[derive(Debug, Clone)]
pub struct DropTracer {
    drop_signal: Weak<u64>,
}

impl DropTracer {
    pub fn all_dropped(&self) -> bool {
        self.drop_signal.strong_count() == 0
    }
}

pub(crate) fn drop_trace() -> (DropTracer, DropMarker) {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let drop_signal = Arc::new(COUNT.fetch_add(1, Ordering::SeqCst));
    (
        DropTracer {
            drop_signal: Arc::downgrade(&drop_signal),
        },
        DropMarker { drop_signal: drop_signal.clone() },
    )
}

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
            pub(crate) fn $setter<M>(&mut self, hook: M)
            where M: Fn(&PluginInstance) -> Result<(), BoxError> + Send + Sync + 'static
            {
                self.hooks.$hook = Some(Box::new(hook))
            }
        )*
    };
}
#[allow(dead_code)]
impl PluginInstance {
    pub(crate) fn mount_at<M: MountPoint>(&mut self, mount_point: &mut M, index: MountPointIndex) -> Result<(), BoxError> {
        let tracer = mount_point.mount(self)?;
        self.mount_points.insert(index, tracer);
        Ok(())
    }
    pub fn snapshot(&self) -> PluginInstanceSnapshot {
        PluginInstanceSnapshot {
            config: self.config.clone(),
            mount_points: self.mount_points.iter().filter_map(|(index, tracer)| if !tracer.all_dropped() { Some(index.clone()) } else { None }).collect(),
        }
    }
    pub(crate) fn call_hook(&self, hook: &Option<PluginInstanceHook>) -> BoxResult<()> {
        if let Some(ref hook) = hook {
            (hook)(self)
        } else {
            Ok(())
        }
    }
    pub fn make(&self) -> BoxLayer {
        BoxLayer::new(FnLayer::new(self.plugin_function.clone()))
    }
    // if we don't clean the mount_points, it may cause a slow memory leak
    // we do it before new instance mounted
    pub(crate) fn mount_points_gc(&mut self) {
        self.mount_points.retain(|_, tracer| !tracer.all_dropped());
    }
    expose_hooks! {
        after_create, set_after_create
        before_mount, set_before_create
        after_mount, set_after_mount
        before_destroy, set_before_destroy
    }
}
