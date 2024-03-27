#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo)]
use std::{
    any::Any,
    borrow::Cow,
    collections::HashMap,
    hash::{DefaultHasher, Hasher},
    sync::{Arc, OnceLock, RwLock},
};

use instance::{PluginInstance, PluginInstanceId, PluginInstanceName, PluginInstanceSnapshot};
use mount::{MountPoint, MountPointIndex};
use rand::random;
use serde::{Deserialize, Serialize};
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
pub mod ext;
pub mod plugins;

#[cfg(feature = "schema")]
pub use schemars;
pub trait Plugin: Any {
    const CODE: &'static str;
    /// is this plugin mono instance
    const MONO: bool = false;
    fn meta() -> PluginMetaData {
        PluginMetaData::default()
    }
    fn create(config: PluginConfig) -> Result<PluginInstance, BoxError>;
    fn create_by_spec(spec: JsonValue, name: Option<String>) -> Result<PluginInstance, BoxError> {
        Self::create(PluginConfig {
            code: Self::CODE.into(),
            spec,
            name,
        })
    }
    fn new_instance<M>(config: PluginConfig, make: M) -> PluginInstance
    where
        M: Fn(&PluginInstance) -> Result<SgBoxLayer, BoxError> + Sync + Send + 'static,
        Self: Sized,
    {
        PluginInstance::new::<Self, _>(config, make)
    }
    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        None
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct PluginMetaData {
    pub author: Option<Cow<'static, str>>,
    pub description: Option<Cow<'static, str>>,
    pub version: Option<Cow<'static, str>>,
    pub homepage: Option<Cow<'static, str>>,
    pub repository: Option<Cow<'static, str>>,
}

/// Plugin Attributes
pub struct PluginAttributes {
    pub mono: bool,
    pub code: Cow<'static, str>,
    pub meta: PluginMetaData,
    #[cfg(feature = "schema")]
    pub schema: Option<schemars::schema::RootSchema>,
    pub constructor: BoxConstructFn,
    pub destructor: Option<BoxDestructFn>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PluginRepoSnapshot {
    pub mono: bool,
    pub code: Cow<'static, str>,
    pub meta: PluginMetaData,
    pub instances: HashMap<PluginInstanceId, PluginInstanceSnapshot>,
}

impl PluginAttributes {
    pub fn from_trait<P: Plugin>() -> Self {
        Self {
            code: P::CODE.into(),
            #[cfg(feature = "schema")]
            schema: P::schema_opt(),
            mono: P::MONO,
            meta: P::meta(),
            constructor: Box::new(P::create),
            destructor: None,
        }
    }
    #[inline]
    pub fn construct(&self, config: PluginConfig) -> Result<PluginInstance, BoxError> {
        (self.constructor)(config)
    }
    pub fn generate_id(&self, config: &PluginConfig) -> PluginInstanceId {
        let name = config.name.as_deref();
        match (name, self.mono) {
            (_, true) => PluginInstanceId {
                code: self.code.clone(),
                name: PluginInstanceName::Mono,
            },
            (Some(name), false) => PluginInstanceId {
                code: self.code.clone(),
                name: PluginInstanceName::Named(name.to_string()),
            },
            (None, false) => {
                let mut hasher = DefaultHasher::new();
                hasher.write(config.code.as_bytes());
                hasher.write(config.spec.to_string().as_bytes());
                let digest = hasher.finish();
                PluginInstanceId {
                    code: self.code.clone(),
                    name: PluginInstanceName::Anon(digest),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfig {
    pub code: Cow<'static, str>,
    pub spec: JsonValue,
    pub name: Option<String>,
}

impl PluginConfig {
    // pub fn instance_id(&self) -> PluginInstanceId {
    //     PluginInstanceId {
    //         code: self.code.clone(),
    //         name: self.name.clone(),
    //     }
    // }
    pub fn check_code<P: Plugin>(&self) -> bool {
        self.code == P::CODE
    }
    pub fn new<P: Plugin>(value: impl Into<JsonValue>) -> Self {
        Self {
            code: P::CODE.into(),
            spec: value.into(),
            ..Default::default()
        }
    }
    pub fn with_name(self, s: impl Into<String>) -> Self {
        Self { name: Some(s.into()), ..self }
    }
    pub fn with_random_name(self) -> Self {
        Self {
            name: Some(format!("{:08x}", random::<u128>())),
            ..self
        }
    }
    pub fn no_name(self) -> Self {
        Self { name: None, ..self }
    }
}

#[cfg(feature = "schema")]
pub trait PluginSchemaExt {
    fn schema() -> schemars::schema::RootSchema;
}

pub trait MakeSgLayer {
    fn make_layer(&self) -> BoxResult<SgBoxLayer>;
}

type BoxConstructFn = Box<dyn Fn(PluginConfig) -> Result<PluginInstance, BoxError> + Send + Sync + 'static>;
type BoxDestructFn = Box<dyn Fn(PluginInstance) -> Result<(), BoxError> + Send + Sync + 'static>;
#[derive(Default, Clone)]
pub struct SgPluginRepository {
    pub plugins: Arc<RwLock<HashMap<Cow<'static, str>, PluginAttributes>>>,
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
        self.register_custom(PluginAttributes::from_trait::<P>())
    }

    pub fn register_custom<A: Into<PluginAttributes>>(&self, attr: A) {
        let attr: PluginAttributes = attr.into();
        let mut map = self.plugins.write().expect("SgPluginRepository register error");
        let _old_attr = map.insert(attr.code.clone(), attr);
    }

    pub fn mount<M: MountPoint>(&self, mount_point: &mut M, mount_index: MountPointIndex, config: PluginConfig) -> Result<(), BoxError> {
        let attr_rg = self.plugins.read().expect("SgPluginRepository register error");
        let code = config.code.clone();
        let Some(attr) = attr_rg.get(&code) else {
            return Err(format!("[Sg.Plugin] unregistered sg plugin type {code}").into());
        };
        let id = attr.generate_id(&config);
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        if let Some(instance) = instances.get_mut(&id) {
            // before mount hook
            instance.before_mount()?;
            instance.mount_at(mount_point, mount_index)?;
            instance.after_mount()?;
            // after mount hook
            Ok(())
        } else {
            tracing::trace!("code: {code}, config {config:?}");
            let mut instance = attr.construct(config)?;
            instance.resource.insert(id.clone());
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

    pub fn repo_snapshot(&self) -> HashMap<Cow<'static, str>, PluginRepoSnapshot> {
        let plugins = self.plugins.read().expect("SgPluginRepository register error");
        plugins
            .iter()
            .map(|(code, attr)| {
                let instances = self.instances.read().expect("SgPluginRepository register error");
                let instances = instances.iter().filter_map(|(id, instance)| if &id.code == code { Some((id.clone(), instance.snapshot())) } else { None }).collect();
                (
                    code.clone(),
                    PluginRepoSnapshot {
                        code: code.clone(),
                        mono: attr.mono,
                        meta: attr.meta.clone(),
                        instances,
                    },
                )
            })
            .collect()
        // self.instances.map
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
    ($CODE:literal, $def:ident, $filter_type:ty $(;$($rest: tt)*)?) => {
        pub const CODE: &str = $CODE;

        #[derive(Debug, Copy, Clone)]
        pub struct $def;

        impl $crate::Plugin for $def {
            const CODE: &'static str = CODE;
            fn create(config: $crate::PluginConfig) -> Result<$crate::instance::PluginInstance, $crate::BoxError> {
                let filter: $filter_type = $crate::serde_json::from_value(config.spec.clone())?;
                let instance = $crate::instance::PluginInstance::new::<Self, _>(config, move |_| $crate::MakeSgLayer::make_layer(&filter));
                Ok(instance)
            }
            $($crate::def_plugin!(@attr $($rest)*);)?
        }
    };
    // finished
    (@attr) => {};
    (@attr $(#[$meta:meta])* mono = $mono: literal; $($rest: tt)*) => {
        const MONO: bool = $mono;
        $crate::def_plugin!(@attr $($rest)*);
    };
    (@attr $(#[$meta:meta])* meta = $metadata: expr; $($rest: tt)*) => {
        fn meta() -> $crate::PluginMetaData {
            $metadata
        }
        $crate::def_plugin!(@attr $($rest)*);
    };
    // enable when schema feature is enabled
    (@attr $(#[$meta:meta])* schema; $($rest: tt)*) => {
        $(#[$meta])*
        fn schema_opt() -> Option<$crate::schemars::schema::RootSchema> {
            Some(<Self as $crate::PluginSchemaExt>::schema())
        }
        $crate::def_plugin!(@attr $($rest)*);
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
    };
}
