use std::{
    any::{Any, TypeId},
    borrow::Cow,
    collections::{HashMap, HashSet},
    convert::Infallible,
    fmt::Display,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use spacegate_kernel::{BoxError, BoxResult, SgBoxLayer};

use crate::{
    mount::{MountPoint, MountPointIndex},
    Plugin, PluginConfig,
};

pub struct PluginInstance {
    // data
    pub config: PluginConfig,
    pub mount_points: HashSet<MountPointIndex>,
    pub hooks: PluginInstanceHooks,
    pub resource: PluginInstanceResource,

    // method
    pub make: BoxMakeFn,
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
    // pub before_drop: Option<PluginInstanceHook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstanceSnapshot {
    pub config: PluginConfig,
    pub mount_points: HashSet<MountPointIndex>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum PluginInstanceName {
    // Anonymous Instance
    Anon(u64),
    // Named Instance
    Named(String),
    // Mono Instance
    Mono,
}

impl Display for PluginInstanceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginInstanceName::Anon(uid) => {
                write!(f, "anon-{:04x}", uid)
            }
            PluginInstanceName::Named(name) => {
                write!(f, "{}", name)
            }
            PluginInstanceName::Mono => {
                write!(f, "*")
            }
        }
    }
}

impl FromStr for PluginInstanceName {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "*" {
            Ok(PluginInstanceName::Mono)
        } else if let Some(anon_id) = s.strip_prefix("anon-").and_then(|uid| u64::from_str_radix(uid, 16).ok()) {
            Ok(PluginInstanceName::Anon(anon_id))
        } else {
            Ok(PluginInstanceName::Named(s.to_string()))
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInstanceId {
    pub code: Cow<'static, str>,
    pub name: PluginInstanceName,
}

impl Serialize for PluginInstanceName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.to_string().as_str())
    }
}

impl<'a> Deserialize<'a> for PluginInstanceName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(PluginInstanceName::from_str(s.as_str()).expect("infallible"))
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
    pub fn new<P: Plugin, M: Fn(&PluginInstance) -> Result<SgBoxLayer, BoxError> + Sync + Send + 'static>(config: PluginConfig, make: M) -> Self {
        Self {
            config: PluginConfig { code: P::CODE.into(), ..config },
            make: Box::new(make),
            mount_points: HashSet::new(),
            hooks: Default::default(),
            resource: Default::default(),
        }
    }
    pub fn make(&self) -> Result<SgBoxLayer, BoxError> {
        (self.make)(self)
    }
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

    expose_hooks! {
        after_create, set_after_create
        before_mount, set_before_create
        after_mount, set_after_mount
        // before_drop
    }
}
