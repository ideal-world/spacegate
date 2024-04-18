#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo)]
use std::{
    any::Any,
    borrow::Cow,
    collections::HashMap,
    hash::{DefaultHasher, Hasher},
    sync::{Arc, OnceLock, RwLock},
};

use futures_util::{future::BoxFuture, Future};
use hyper::{Request, Response};
use instance::{PluginInstance, PluginInstanceId, PluginInstanceName, PluginInstanceSnapshot};
use layer::{InnerBoxPf, PluginFunction};
use mount::{MountPoint, MountPointIndex};
use rand::random;
use serde::{Deserialize, Serialize};
pub use serde_json;
pub use serde_json::{Error as SerdeJsonError, Value as JsonValue};
pub use spacegate_kernel::helper_layers::filter::{Filter, FilterRequest, FilterRequestLayer};
pub use spacegate_kernel::helper_layers::function::Inner;
pub use spacegate_kernel::BoxError;
use spacegate_kernel::SgBody;
pub use spacegate_kernel::SgBoxLayer;
pub mod error;
pub mod model;
pub mod mount;
// pub mod plugins;
pub mod instance;
pub use error::PluginError;
pub mod ext;
pub mod layer;
pub mod plugins;

#[cfg(feature = "schema")]
pub use schemars;
pub trait Plugin: Any + Sized + Send + Sync {
    const CODE: &'static str;
    /// is this plugin mono instance
    const MONO: bool = false;
    fn meta() -> PluginMetaData {
        PluginMetaData::default()
    }
    fn call(&self, req: Request<SgBody>, inner: Inner) -> impl Future<Output = Result<Response<SgBody>, BoxError>> + Send;
    fn create(plugin_config: PluginConfig) -> Result<Self, BoxError>;
    fn create_by_spec(spec: JsonValue, name: Option<String>) -> Result<Self, BoxError> {
        Self::create(PluginConfig {
            code: Self::CODE.into(),
            spec,
            name,
        })
    }
    fn register(repo: &SgPluginRepository) {
        repo.plugins.write().expect("SgPluginRepository register error").insert(Self::CODE.into(), PluginAttributes::from_trait::<Self>());
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
    pub make_pf: BoxMakePfMethod,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct PluginRepoSnapshot {
    pub mono: bool,
    pub code: Cow<'static, str>,
    pub meta: PluginMetaData,
    pub instances: HashMap<PluginInstanceName, PluginInstanceSnapshot>,
}

impl PluginAttributes {
    pub fn from_trait<P: Plugin>() -> Self {
        let constructor = move |config: PluginConfig| {
            let plugin = Arc::new(P::create(config)?);
            let function = move |req: Request<SgBody>, inner: Inner| {
                let plugin = plugin.clone();
                let task = async move {
                    match plugin.call(req, inner).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            tracing::error!("plugin error: {e}");
                            PluginError::internal_error::<P>(e).into()
                        }
                    }
                };
                Box::pin(task) as BoxFuture<'static, Response<SgBody>>
            };
            Ok(Box::new(function) as InnerBoxPf)
        };
        Self {
            code: P::CODE.into(),
            #[cfg(feature = "schema")]
            schema: P::schema_opt(),
            mono: P::MONO,
            meta: P::meta(),
            make_pf: Box::new(constructor),
        }
    }
    #[inline]
    pub(crate) fn make_pf(&self, config: PluginConfig) -> Result<InnerBoxPf, BoxError> {
        (self.make_pf)(config)
    }
    pub fn generate_id(&self, config: &PluginConfig) -> PluginInstanceId {
        if self.mono {
            PluginInstanceId {
                code: self.code.clone(),
                name: PluginInstanceName::Mono,
            }
        } else {
            config.none_mono_id()
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
    pub fn digest(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(self.code.as_bytes());
        hasher.write(self.spec.to_string().as_bytes());
        hasher.finish()
    }
    pub fn none_mono_instance_name(&self) -> PluginInstanceName {
        let digest = self.digest();
        if let Some(name) = self.name.as_ref() {
            PluginInstanceName::Named(name.clone())
        } else {
            PluginInstanceName::Anon(digest)
        }
    }
    pub fn none_mono_id(&self) -> PluginInstanceId {
        PluginInstanceId {
            code: self.code.clone(),
            name: self.none_mono_instance_name(),
        }
    }
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

type BoxMakePfMethod = Box<dyn Fn(PluginConfig) -> Result<InnerBoxPf, BoxError> + Send + Sync + 'static>;
type _BoxDestructFn = Box<dyn Fn(PluginInstance) -> Result<(), BoxError> + Send + Sync + 'static>;
#[derive(Default, Clone)]
pub struct SgPluginRepository {
    pub plugins: Arc<RwLock<HashMap<Cow<'static, str>, PluginAttributes>>>,
    pub instances: Arc<RwLock<HashMap<PluginInstanceId, PluginInstance>>>,
}

pub struct PluginInstanceRef {
    pub id: PluginInstanceId,
    pub digest: u64,
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
        #[cfg(feature = "redis")]
        {
            self.register::<ext::redis::plugins::redis_count::RedisCountPlugin>();
            self.register::<ext::redis::plugins::redis_limit::RedisLimitPlugin>();
            self.register::<ext::redis::plugins::redis_time_range::RedisTimeRangePlugin>();
            self.register::<ext::redis::plugins::redis_dynamic_route::RedisDynamicRoutePlugin>();
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

    pub fn clear_routes_instances(&self, gateway: &str) {
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        instances.clear();
    }

    pub fn clear_gateway_instances(&self, gateway: &str) {
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        instances.clear();
    }

    pub fn create_or_update_instance(&self, config: PluginConfig) -> Result<(), BoxError> {
        let attr_rg = self.plugins.read().expect("SgPluginRepository register error");
        let code = config.code.clone();
        let Some(attr) = attr_rg.get(&code) else {
            return Err(format!("[Sg.Plugin] unregistered sg plugin type {code}").into());
        };
        let id = attr.generate_id(&config);
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        if let Some(instance) = instances.get_mut(&id) {
            let new_inner_pf = attr.make_pf(config)?;
            instance.plugin_function.swap(new_inner_pf);
        } else {
            let pf = PluginFunction::new(attr.make_pf(config.clone())?);
            let instance = PluginInstance {
                config,
                plugin_function: pf,
                resource: Default::default(),
                mount_points: Default::default(),
                hooks: Default::default(),
            };
            instance.after_create()?;
            instances.insert(id, instance);
        }
        Ok(())
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
            // todo!("check if config has changed");
            // before mount hook
            instance.before_mount()?;
            instance.mount_at(mount_point, mount_index)?;
            instance.after_mount()?;
            // after mount hook
            Ok(())
        } else {
            Err(format!("[Sg.Plugin] missing instance {id:?}").into())
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
                let instances = instances.iter().filter_map(|(id, instance)| if &id.code == code { Some((id.name.clone(), instance.snapshot())) } else { None }).collect();
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
