#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo)]
use std::{
    any::Any,
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};

use instance::{PluginInstance, PluginInstanceId, PluginInstanceSnapshot};
use mount::{MountPoint, MountPointIndex};
pub use serde_json;
pub use serde_json::{Error as SerdeJsonError, Value as JsonValue};
pub use spacegate_kernel::helper_layers::filter::{Filter, FilterRequest, FilterRequestLayer};
use spacegate_kernel::BoxResult;
pub use spacegate_kernel::SgBoxLayer;

pub use spacegate_kernel::BoxError;
pub mod error;
pub mod model;
pub mod mount;
// pub mod plugins;
pub mod instance;
pub use error::PluginError;
pub mod plugins;

#[cfg(feature = "schema")]
pub use schemars;
pub trait Plugin: Any {
    const CODE: &'static str;
    fn create(config: PluginConfig) -> Result<PluginInstance, BoxError>;
}

#[derive(Debug, Clone, Default)]
pub struct PluginConfig {
    pub code: Cow<'static, str>,
    pub spec: JsonValue,
    pub name: Option<String>,
}

impl PluginConfig {
    pub fn instance_id(&self) -> PluginInstanceId {
        PluginInstanceId {
            code: self.code.clone(),
            name: self.name.clone(),
        }
    }
}

#[cfg(feature = "schema")]
pub trait PluginSchemaExt {
    fn schema() -> schemars::schema::RootSchema;
}

pub trait MakeSgLayer {
    fn make_layer(&self) -> BoxResult<SgBoxLayer>;
}

type BoxCreateFn = Box<dyn Fn(PluginConfig) -> Result<PluginInstance, BoxError> + Send + Sync + 'static>;
#[derive(Default, Clone)]
pub struct SgPluginRepository {
    pub creators: Arc<RwLock<HashMap<Cow<'static, str>, BoxCreateFn>>>,
    pub instances: Arc<RwLock<HashMap<PluginInstanceId, PluginInstance>>>,
}

impl SgPluginRepository {
    pub fn global() -> &'static Self {
        static INIT: OnceLock<SgPluginRepository> = OnceLock::new();
        INIT.get_or_init(|| {
            let repo = SgPluginRepository::new();
            repo.register_prelude();
            repo
        })
    }

    pub fn register_prelude(&self) {
        // self.register::<plugins::static_resource::StaticResourcePlugin>();
        #[cfg(feature = "limit")]
        self.register::<plugins::limit::RateLimitPlugin>();
        #[cfg(feature = "redirect")]
        self.register::<plugins::redirect::RedirectPlugin>();
        #[cfg(feature = "retry")]
        self.register::<plugins::retry::RetryPlugin>();
        #[cfg(feature = "header-modifier")]
        self.register::<plugins::header_modifier::HeaderModifierPlugin>();
        #[cfg(feature = "inject")]
        self.register::<plugins::inject::InjectPlugin>();
        #[cfg(feature = "rewrite")]
        self.register::<plugins::rewrite::RewritePlugin>();
        #[cfg(feature = "maintenance")]
        self.register::<plugins::maintenance::MaintenancePlugin>();
        // #[cfg(feature = "status")]
        // self.register::<plugins::status::StatusPlugin>();
        #[cfg(feature = "decompression")]
        self.register::<plugins::decompression::DecompressionPlugin>();
        #[cfg(feature = "redis")]
        {
            self.register::<plugins::redis::redis_count::RedisCountPlugin>();
            self.register::<plugins::redis::redis_limit::RedisLimitPlugin>();
            self.register::<plugins::redis::redis_time_range::RedisTimeRangePlugin>();
            self.register::<plugins::redis::redis_dynamic_route::RedisDynamicRoutePlugin>();
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<P: Plugin>(&self) {
        self.register_fn(P::CODE, P::create)
    }

    pub fn register_fn<C, F>(&self, code: C, create: F)
    where
        C: Into<Cow<'static, str>>,
        F: Fn(PluginConfig) -> Result<PluginInstance, BoxError> + Send + Sync + 'static,
    {
        let mut map = self.creators.write().expect("SgPluginRepository register error");
        let create_fn = Box::new(create);
        map.insert(code.into(), create_fn);
    }

    pub fn mount<M: MountPoint>(&self, mount_point: &mut M, mount_index: MountPointIndex, config: PluginConfig) -> Result<(), BoxError> {
        let map = self.creators.read().expect("SgPluginRepository register error");
        let code: Cow<'static, str> = config.code.clone();
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        let id = PluginInstanceId {
            code: code.clone(),
            name: config.name.clone(),
        };
        if let Some(instance) = instances.get_mut(&id) {
            // before mount hook
            instance.before_mount()?;
            instance.mount_at(mount_point, mount_index)?;
            instance.after_mount()?;
            // after mount hook
            Ok(())
        } else {
            let creator = map.get(&code).ok_or_else::<BoxError, _>(|| format!("[Sg.Plugin] unregistered sg plugin type {code}").into())?;
            let mut instance = (creator)(config)?;
            instance.after_create()?;
            // after create hook
            // before mount hook
            instance.before_mount()?;
            instance.mount_at(mount_point, mount_index)?;
            // after mount hook
            instance.after_mount()?;
            instances.insert(id.clone(), instance);
            Ok(())
        }
    }

    pub fn instance_snapshot(&self, id: PluginInstanceId) -> Option<PluginInstanceSnapshot> {
        let map = self.instances.read().expect("SgPluginRepository register error");
        map.get(&id).map(PluginInstance::snapshot)
    }
}

/// # Generate plugin definition
/// ## Concept Note
/// ### Plugin definition
/// Plugin definitions are used to register
///
/// ## Parameter Description
/// ### code
/// Defines a unique code for a plugins, used to specify this code in
/// the configuration to use this plug-in
/// ### def
/// The recommended naming convention is `{filter_type}Def`
/// ### filter_type
/// Actual struct of Filter
#[macro_export]
macro_rules! def_plugin {
    ($CODE:literal, $def:ident, $filter_type:ty) => {
        pub const CODE: &str = $CODE;

        #[derive(Debug, Copy, Clone)]
        pub struct $def;

        impl $crate::Plugin for $def {
            const CODE: &'static str = CODE;
            fn create(config: $crate::PluginConfig) -> Result<$crate::PluginInstance, $crate::BoxError> {
                let filter: $filter_type = $crate::serde_json::from_value(config.spec.clone())?;
                let instance = $crate::instance::PluginInstance::new::<Self, _>(config, move || $crate::MakeSgLayer::make_layer(&filter));
                Ok(instance)
            }
        }
    };
}

#[cfg(feature = "schema")]
#[macro_export]
macro_rules! schema {
    ($plugin:ident, $schema:ty) => {
        impl $crate::PluginSchemaExt for $plugin {
            fn schema() -> $crate::schemars::schema::RootSchema {
                $crate::schemars::schema_for!($schema)
            }
        }
    };
    ($plugin:ident, $schema:expr) => {
        impl $crate::PluginSchemaExt for $plugin {
            fn schema() -> $crate::schemars::schema::RootSchema {
                $crate::schemars::schema_for_value!($schema)
            }
        }
    }; // ($plugin:ident) => {
       //     impl $crate::PluginSchemaExt for $plugin {
       //         fn schema() -> $crate::schemars::schema::RootSchema {
       //             $crate::schemars::schema_for!(<$plugin as $crate::Plugin>::MakeLayer)
       //         }
       //     }
       // };
}
