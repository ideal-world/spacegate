use std::{any::TypeId, borrow::Cow, collections::HashSet};

use spacegate_kernel::{BoxError, BoxResult, SgBoxLayer};

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
    pub hooks: PluginInstanceHooks,
}

type PluginInstanceHook = Box<dyn Fn(&PluginInstance) -> Result<(), BoxError> + Send + Sync + 'static>;
#[derive(Default)]
pub struct PluginInstanceHooks {
    pub after_create: Option<PluginInstanceHook>,
    pub before_mount: Option<PluginInstanceHook>,
    pub after_mount: Option<PluginInstanceHook>,
    // pub before_drop: Option<PluginInstanceHook>,
}

#[derive(Debug, Clone)]
pub struct PluginInstanceSnapshot {
    pub plugin: TypeId,
    pub config: PluginConfig,
    pub mount_points: HashSet<MountPointIndex>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct PluginInstanceId {
    pub code: Cow<'static, str>,
    pub name: Option<String>,
}

impl PluginInstanceId {
    #[cfg(feature = "redis")]
    pub fn redis_prefix(&self) -> String {
        let id = self.name.as_deref().unwrap_or("*");
        let code = self.code.as_ref();
        format!("sg:plugin:{code}:{id}")
    }
    #[cfg(feature = "axum")]
    pub fn route(&self, router: spacegate_ext_axum::axum::Router) {
        let server = spacegate_ext_axum::AxumServer::global();
        let mut wg = server.blocking_write();
        let path = format!("/plugin/{code}/instance/{name}", code = self.code, name = self.name.as_deref().unwrap_or("*"));
        let mut swap_out = spacegate_ext_axum::axum::Router::default();
        std::mem::swap(&mut swap_out, &mut wg.router);
        wg.router = swap_out.nest(&path, router);
    }
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
    pub fn new<P: Plugin, M: Fn() -> Result<SgBoxLayer, BoxError> + Sync + Send + 'static>(config: PluginConfig, make: M) -> Self {
        Self {
            plugin: TypeId::of::<P>(),
            config: PluginConfig { code: P::CODE.into(), ..config },
            make: Box::new(make),
            mount_points: HashSet::new(),
            hooks: Default::default(),
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
    pub(crate) fn call_hook(&self, hook: &Option<PluginInstanceHook>) -> BoxResult<()> {
        if let Some(ref hook) = hook {
            (hook)(self)
        } else {
            Ok(())
        }
    }

    expose_hooks! {
        after_create, set_after_create
        before_mount, set_before_create
        after_mount, set_after_mount
        // before_drop
    }
}
