#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo)]
use std::{
    any::{Any, TypeId},
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock, RwLock, Weak},
};

use instance::{PluginIncetanceId, PluginInstance, PluginInstanceSnapshot};
use mount::{MountPoint, MountPointIndex};
pub use serde_json;
pub use serde_json::{Error as SerdeJsonError, Value as JsonValue};
pub use spacegate_kernel::helper_layers::filter::{Filter, FilterRequest, FilterRequestLayer};
pub use spacegate_kernel::SgBoxLayer;
use spacegate_kernel::{
    layers::{
        gateway::builder::SgGatewayLayerBuilder,
        http_route::builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
    },
    BoxResult,
};

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
    // fn redis_prefix(id: Option<&str>) -> String {
    //     let id = id.unwrap_or("*");
    //     format!("sg:plugin:{code}:{id}", code = Self::CODE)
    // }
}

#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub code: String,
    pub spec: JsonValue,
    pub name: Option<String>,
}

#[cfg(feature = "schema")]
pub trait PluginSchemaExt {
    fn schema() -> schemars::schema::RootSchema;
}

pub trait MakeSgLayer {
    fn make_layer(&self) -> BoxResult<SgBoxLayer>;
    fn install_on_gateway(&self, gateway: &mut SgGatewayLayerBuilder) -> Result<(), BoxError> {
        let layer = self.make_layer()?;
        gateway.http_plugins.push(layer);
        Ok(())
    }
    fn install_on_backend(&self, backend: &mut SgHttpBackendLayerBuilder) -> Result<(), BoxError> {
        let layer = self.make_layer()?;
        backend.plugins.push(layer);
        Ok(())
    }
    fn install_on_route(&self, route: &mut SgHttpRouteLayerBuilder) -> Result<(), BoxError> {
        let layer = self.make_layer()?;
        route.plugins.push(layer);
        Ok(())
    }
    fn install_on_rule(&self, rule: &mut SgHttpRouteRuleLayerBuilder) -> Result<(), BoxError> {
        let layer = self.make_layer()?;
        rule.plugins.push(layer);
        Ok(())
    }
}

type BoxCreateFn = Box<dyn Fn(PluginConfig) -> Result<PluginInstance, BoxError> + Send + Sync + 'static>;
#[derive(Default, Clone)]
pub struct SgPluginRepository {
    pub creators: Arc<RwLock<HashMap<Cow<'static, str>, BoxCreateFn>>>,
    pub instances: Arc<RwLock<HashMap<PluginIncetanceId, PluginInstance>>>,
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
        let code: Cow<'static, str> = config.code.clone().into();
        let mut instances = self.instances.write().expect("SgPluginRepository register error");
        let id = PluginIncetanceId {
            code: code.clone(),
            name: config.name.clone(),
        };
        if let Some(instance) = instances.get_mut(&id) {
            instance.mount_at(mount_point, mount_index)?;
            Ok(())
        } else {
            let creator = map.get(&code).ok_or_else::<BoxError, _>(|| format!("[Sg.Plugin] unregistered sg plugin type {code}").into())?;
            let mut instance = (creator)(config)?;
            instance.mount_at(mount_point, mount_index)?;
            instances.insert(id.clone(), instance);
            Ok(())
        }
    }

    pub fn instance_snapshot(&self, id: PluginIncetanceId) -> Option<PluginInstanceSnapshot> {
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

        pub struct $def;

        impl $crate::Plugin for $def {
            const CODE: &'static str = CODE;
            type MakeLayer = $filter_type;
            fn create(_name: Option<String>, value: $crate::JsonValue) -> Result<Self::MakeLayer, $crate::BoxError> {
                let filter: $filter_type = $crate::serde_json::from_value(value)?;
                Ok(filter)
            }
        }
    };
}

/// # Define Plugin Filter
///
/// use `def_filter_plugin` macro to define a filter plugin for an exsited struct which implemented [Filter](spacegate_kernel::helper_layers::filter::Filter).
///
/// ```
/// # use serde::{Serialize, Deserialize};
/// # use hyper::{http::{StatusCode, header::AUTHORIZATION}, Response, Request};
/// # use spacegate_kernel::{SgResponseExt, SgBody};
/// # use spacegate_plugin::{def_filter_plugin, Filter, MakeSgLayer, SgBoxLayer};
/// #[derive(Default, Debug, Serialize, Deserialize, Clone)]
/// pub struct SgFilterAuth {}
///
/// impl Filter for SgFilterAuth {
///     fn filter(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
///         if req.headers().contains_key(AUTHORIZATION) {
///             Ok(req)
///         } else {
///             Err(Response::with_code_message(StatusCode::UNAUTHORIZED, "missing authorization header"))
///         }
///     }
/// }
///
/// def_filter_plugin!("auth", SgFilterAuthPlugin, SgFilterAuth);
/// ```

#[macro_export]
macro_rules! def_filter_plugin {
    ($CODE:literal, $def:ident, $filter_type:ty) => {
        pub const CODE: &str = $CODE;

        pub struct $def;

        impl $crate::Plugin for $def {
            const CODE: &'static str = CODE;
            type MakeLayer = $filter_type;
            fn create(_name: Option<String>, value: $crate::JsonValue) -> Result<Self::MakeLayer, $crate::BoxError> {
                let filter: $filter_type = $crate::serde_json::from_value(value)?;
                Ok(filter)
            }
        }

        impl $crate::MakeSgLayer for $filter_type {
            fn make_layer(&self) -> Result<$crate::SgBoxLayer, $crate::BoxError> {
                let layer = $crate::FilterRequestLayer::new(self.clone());
                Ok($crate::SgBoxLayer::new(layer))
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
    };
    ($plugin:ident) => {
        impl $crate::PluginSchemaExt for $plugin {
            fn schema() -> $crate::schemars::schema::RootSchema {
                $crate::schemars::schema_for!(<$plugin as $crate::Plugin>::MakeLayer)
            }
        }
    };
}
