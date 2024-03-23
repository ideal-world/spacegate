#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo)]
use std::{
    any::TypeId,
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock, Weak},
};

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
pub mod plugins;
pub mod mount;
pub use error::PluginError;

/// 我们的设计应该是无状态的函数式的。
#[cfg(feature = "schema")]
pub use schemars;
pub trait Plugin {
    type MakeLayer: MakeSgLayer + 'static;
    const CODE: &'static str;
    fn create(id: Option<String>, value: JsonValue) -> Result<Self::MakeLayer, BoxError>;
    fn redis_prefix(id: Option<&str>) -> String {
        let id = id.unwrap_or("*");
        format!("sg:plugin:{code}:{id}", code = Self::CODE)
    }
}

pub struct PluginInstance {
    pub plugin: TypeId,
    pub code: &'static str,
    pub id: Option<String>,
    pub global_id: u128,
    pub layer: SgBoxLayer,
    pub mount_point: Option<Weak<dyn MountPoint>>,
    pub hooks: PluginInstanceHooks,
}

pub struct PluginInstanceHooks {
    pub before_mount: Option<Box<dyn Fn(&PluginInstance)>>,
    pub after_mount: Option<Box<dyn Fn(&PluginInstance)>>,
    pub before_unmount: Option<Box<dyn Fn(&PluginInstance)>>,
    pub after_unmount: Option<Box<dyn Fn(&PluginInstance)>>,
}

impl PluginInstance {}

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

type BoxCreateFn = Box<dyn Fn(Option<String>, JsonValue) -> Result<Box<dyn MakeSgLayer>, BoxError> + Send + Sync>;
#[derive(Default, Clone)]
pub struct SgPluginRepository {
    pub map: Arc<RwLock<HashMap<&'static str, BoxCreateFn>>>,
}

pub trait MountPoint {
    fn name(&self) -> String;
    fn mount(&mut self, instance: PluginInstance);
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
        self.register::<plugins::static_resource::StaticResourcePlugin>();
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
        let mut map = self.map.write().expect("SgPluginTypeMap register error");
        let create_fn = Box::new(move |id: Option<String>, value| P::create(id, value).map_err(BoxError::from).map(|x| Box::new(x) as Box<dyn MakeSgLayer>));
        map.insert(P::CODE, Box::new(create_fn));
    }

    pub fn create(&self, name: Option<String>, code: &str, value: JsonValue) -> Result<Box<dyn MakeSgLayer>, BoxError> {
        let map = self.map.read().expect("SgPluginTypeMap register error");
        if let Some(t) = map.get(code) {
            (t)(name, value)
        } else {
            Err(format!("[Sg.Plugin] unregistered sg plugin type {code}").into())
        }
    }

    pub fn create_layer(&self, name: Option<String>, code: &str, value: JsonValue) -> Result<SgBoxLayer, BoxError> {
        let inner = self.create(name, code, value)?.make_layer()?;
        Ok(inner)
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
