#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo, clippy::inline_always)]
use std::{
    any::Any,
    borrow::Cow,
    collections::{HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, OnceLock, RwLock},
};

use futures_util::{future::BoxFuture, Future};
use hyper::{Request, Response};
use instance::{PluginInstance, PluginInstanceSnapshot};
use layer::{InnerBoxPf, PluginFunction};
use mount::{MountPoint, MountPointIndex};
use serde::{Deserialize, Serialize};
pub use serde_json;
pub use serde_json::{Error as SerdeJsonError, Value as JsonValue};
pub use spacegate_kernel::helper_layers::function::Inner;
pub use spacegate_kernel::BoxError;
pub use spacegate_kernel::BoxLayer;
pub use spacegate_kernel::{SgBody, SgRequest, SgRequestExt, SgResponse, SgResponseExt};
pub mod error;
pub mod instance;
pub mod model;
pub mod mount;
pub use error::PluginError;
#[cfg(feature = "dylib")]
pub mod dynamic;
pub mod ext;
pub mod layer;
pub mod plugins;
#[cfg(feature = "schema")]
pub use schemars;
pub use spacegate_model;
pub use spacegate_model::{plugin_meta, PluginAttributes, PluginConfig, PluginInstanceId, PluginInstanceMap, PluginInstanceName, PluginMetaData};
/// # Plugin Trait
/// It's a easy way to define a plugin through this trait.
/// You should give a unique [`code`](Plugin::CODE) for the plugin,
/// and implement the [`call`](Plugin::call) function and the [`create`](Plugin::create) function.
///
/// # Example
/// In the follow example, we add a server header for each response.
/// ```rust
/// # use spacegate_plugin::{Plugin, SgRequest, SgResponse, Inner, BoxError, PluginConfig};
/// pub struct ServerHeaderPlugin {
///     header_value: String,
/// }
///
/// impl Plugin for ServerHeaderPlugin {
///     const CODE: &'static str = "server-header";
///     async fn call(&self, req: SgRequest, inner: Inner) -> Result<SgResponse, BoxError> {
///         let mut resp = inner.call(req).await;    
///         resp.headers_mut().insert("server", self.header_value.parse()?);
///         Ok(resp)
///     }
///     fn create(plugin_config: PluginConfig) -> Result<Self, BoxError> {
///         let Some(header_value) = plugin_config.spec.get("header_value") else {
///             return Err("missing header_value".into())
///         };
///         Ok(Self {
///            header_value: header_value.as_str().unwrap_or("spacegate").to_string(),
///         })
///     }
/// }
/// ```
pub trait Plugin: Any + Sized + Send + Sync {
    /// Plugin code, it should be unique repository-wise.
    /// 
    /// It's **recommended** to use a **kebab-case** string, witch would make k8s happy.
    const CODE: &'static str;
    /// is this plugin mono instance
    const MONO: bool = false;
    fn meta() -> PluginMetaData {
        PluginMetaData::default()
    }
    /// This function will be called when the plugin is invoked.
    ///
    /// The error will be wrapped with a response with status code 500, and the error message will be response's body.
    ///
    /// If you want to return a custom response, wrap it with `Ok` and return it.
    ///
    /// If you want to return a error response with other status code, use `PluginError::new` to create a error response, and wrap
    /// it with `Ok`.
    fn call(&self, req: SgRequest, inner: Inner) -> impl Future<Output = Result<SgResponse, BoxError>> + Send;
    fn create(plugin_config: PluginConfig) -> Result<Self, BoxError>;
    fn create_by_spec(spec: JsonValue, name: PluginInstanceName) -> Result<Self, BoxError> {
        Self::create(PluginConfig {
            id: PluginInstanceId { code: Self::CODE.into(), name },
            spec,
        })
    }
    /// Register the plugin to the repository.
    ///
    /// You can also register axum server route here.
    fn register(repo: &PluginRepository) {
        repo.plugins.write().expect("SgPluginRepository register error").insert(Self::CODE.into(), PluginDefinitionObject::from_trait::<Self>());
    }

    #[cfg(feature = "schema")]
    /// Return the schema of the plugin config.
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        None
    }
}

/// Plugin Trait Object
pub struct PluginDefinitionObject {
    pub mono: bool,
    pub code: Cow<'static, str>,
    pub meta: PluginMetaData,
    #[cfg(feature = "schema")]
    pub schema: Option<schemars::schema::RootSchema>,
    pub make_pf: Box<MakePfMethod>,
}

impl Debug for PluginDefinitionObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut formatter = f.debug_struct("PluginAttributes");
        formatter.field("mono", &self.mono).field("code", &self.code).field("meta", &self.meta);
        #[cfg(feature = "schema")]
        {
            formatter.field("schema", &self.schema.is_some());
        }

        formatter.finish()
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PluginRepoSnapshot {
    pub mono: bool,
    pub code: Cow<'static, str>,
    pub meta: PluginMetaData,
    pub instances: HashMap<String, PluginInstanceSnapshot>,
}

impl PluginDefinitionObject {
    pub fn attr(&self) -> PluginAttributes {
        PluginAttributes {
            mono: self.mono,
            code: self.code.clone(),
            meta: self.meta.clone(),
        }
    }
    pub fn from_trait<P: Plugin>() -> Self {
        let constructor = move |config: PluginConfig| {
            let plugin = Arc::new(P::create(config)?);
            let function = move |req: Request<SgBody>, inner: Inner| {
                let plugin = plugin.clone();
                // let plugin_span = tracing::span!(tracing::Level::INFO, "plugin", code = P::CODE);
                let task = async move {
                    // let _entered = plugin_span.enter();
                    match plugin.call(req, inner).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            tracing::error!("{code} plugin error: {e}", code = P::CODE);
                            PluginError::internal_error::<P>(e).into()
                        }
                    }
                };
                Box::pin(task) as BoxFuture<'static, Response<SgBody>>
            };
            Ok(Box::new(function) as InnerBoxPf)
        };
        let make_pf = Box::new(constructor);
        Self {
            code: P::CODE.into(),
            #[cfg(feature = "schema")]
            schema: P::schema_opt(),
            mono: P::MONO,
            meta: P::meta(),
            make_pf,
        }
    }
    #[inline]
    pub(crate) fn make_pf(&self, config: PluginConfig) -> Result<InnerBoxPf, BoxError> {
        (self.make_pf)(config)
    }
}

#[cfg(feature = "schema")]
pub trait PluginSchemaExt {
    fn schema() -> schemars::schema::RootSchema;
}

pub type MakePfMethod = dyn Fn(PluginConfig) -> Result<InnerBoxPf, BoxError> + Send + Sync + 'static;

/// # Plugin Repository
/// A repository to manage plugins, it stores plugin definitions and instances.
///
/// You can get a global instance through [`PluginRepository::global`].
#[derive(Default, Clone)]
pub struct PluginRepository {
    plugins: Arc<RwLock<HashMap<String, PluginDefinitionObject>>>,
    instances: Arc<RwLock<HashMap<PluginInstanceId, PluginInstance>>>,
}

impl PluginRepository {
    /// Get a global instance of this repository.
    ///
    /// Once the repository is initialized, it will register all plugins in this crate.
    pub fn global() -> &'static Self {
        static INIT: OnceLock<PluginRepository> = OnceLock::new();
        INIT.get_or_init(|| {
            let repo = PluginRepository::new();
            repo.register_prelude();
            repo
        })
    }

    /// register all plugins in this crates
    pub fn register_prelude(&self) {
        self.register::<plugins::static_resource::StaticResourcePlugin>();
        #[cfg(feature = "limit")]
        self.register::<plugins::limit::RateLimitPlugin>();
        #[cfg(feature = "redirect")]
        self.register::<plugins::redirect::RedirectPlugin>();
        // #[cfg(feature = "retry")]
        // self.register::<plugins::retry::RetryPlugin>();
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
        #[cfg(feature = "set-version")]
        self.register::<plugins::set_version::SetVersionPlugin>();
        #[cfg(feature = "set-scheme")]
        self.register::<plugins::set_scheme::SetSchemePlugin>();
        #[cfg(feature = "redis")]
        {
            self.register::<ext::redis::plugins::redis_count::RedisCountPlugin>();
            self.register::<ext::redis::plugins::redis_limit::RedisLimitPlugin>();
            self.register::<ext::redis::plugins::redis_time_range::RedisTimeRangePlugin>();
            self.register::<ext::redis::plugins::redis_dynamic_route::RedisDynamicRoutePlugin>();
        }
    }

    /// create a new empty repository
    pub fn new() -> Self {
        Self::default()
    }

    /// register by [`Plugin`] trait
    pub fn register<P: Plugin>(&self) {
        self.register_custom(PluginDefinitionObject::from_trait::<P>())
    }

    /// register a custom plugin
    pub fn register_custom<A: Into<PluginDefinitionObject>>(&self, attr: A) {
        let attr: PluginDefinitionObject = attr.into();
        let mut map = self.plugins.write().expect("SgPluginRepository register error");
        let _old_attr = map.insert(attr.code.to_string(), attr);
    }

    /// clear all instances
    pub fn clear_instances(&self) {
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        for (_, inst) in instances.drain() {
            if let Err(e) = inst.before_destroy() {
                tracing::error!("plugin {id:?} before_destroy error: {e}", id = inst.config.id, e = e);
            }
        }
    }

    /// create or update a plugin instance by config
    pub fn create_or_update_instance(&self, config: PluginConfig) -> Result<(), BoxError> {
        let attr_rg = self.plugins.read().expect("SgPluginRepository register error");
        let code = config.code();
        let id = config.id.clone();
        let Some(attr) = attr_rg.get(code) else {
            return Err(format!("[Sg.Plugin] unregistered sg plugin type {code}").into());
        };
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        if let Some(instance) = instances.get_mut(&id) {
            let new_inner_pf = attr.make_pf(config)?;
            instance.plugin_function.swap(new_inner_pf);
        } else {
            let pf = PluginFunction::new(attr.make_pf(config.clone())?);
            let instance = PluginInstance {
                config,
                plugin_function: pf,
                mount_points: Default::default(),
                hooks: Default::default(),
            };
            instance.after_create()?;
            instances.insert(id, instance);
        }
        Ok(())
    }

    /// remove a plugin instance by id
    pub fn remove_instance(&self, id: &PluginInstanceId) -> Result<HashSet<MountPointIndex>, BoxError> {
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        if let Some(instance) = instances.remove(id) {
            instance.before_destroy()?;
            Ok(instance.mount_points.into_iter().filter_map(|(index, tracer)| (!tracer.all_dropped()).then_some(index)).collect())
        } else {
            Err(format!("[Sg.Plugin] missing instance {id:?}").into())
        }
    }

    /// mount a plugin instance to a mount point
    pub fn mount<M: MountPoint>(&self, mount_point: &mut M, mount_index: MountPointIndex, id: PluginInstanceId) -> Result<(), BoxError> {
        let attr_rg = self.plugins.read().expect("SgPluginRepository register error");
        let code = id.code.as_ref();
        let Some(_attr) = attr_rg.get(code) else {
            return Err(format!("[Sg.Plugin] unregistered sg plugin type {code}").into());
        };
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        if let Some(instance) = instances.get_mut(&id) {
            instance.mount_points_gc();
            // before mount hook
            instance.before_mount()?;
            instance.mount_at(mount_point, mount_index)?;
            // after mount hook
            instance.after_mount()?;
            Ok(())
        } else {
            Err(format!("[Sg.Plugin] missing instance {id:?}").into())
        }
    }

    pub fn instance_snapshot(&self, id: PluginInstanceId) -> Option<PluginInstanceSnapshot> {
        let map = self.instances.read().expect("SgPluginRepository register error");
        map.get(&id).map(PluginInstance::snapshot)
    }

    pub fn plugin_list(&self) -> Vec<PluginAttributes> {
        let map = self.plugins.read().expect("SgPluginRepository register error");
        map.values().map(PluginDefinitionObject::attr).collect()
    }

    pub fn repo_snapshot(&self) -> HashMap<String, PluginRepoSnapshot> {
        let plugins = self.plugins.read().expect("SgPluginRepository register error");
        plugins
            .iter()
            .map(|(code, attr)| {
                let instances = self.instances.read().expect("SgPluginRepository register error");
                let instances = instances.iter().filter_map(|(id, instance)| if &id.code == code { Some((id.name.to_string(), instance.snapshot())) } else { None }).collect();
                (
                    code.clone(),
                    PluginRepoSnapshot {
                        code: code.clone().into(),
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
