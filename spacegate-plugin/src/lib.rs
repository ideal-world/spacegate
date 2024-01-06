#![deny(clippy::unwrap_used, clippy::dbg_macro, clippy::unimplemented, clippy::todo)]
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};

pub use spacegate_tower::helper_layers::filter::{Filter, FilterRequest, FilterRequestLayer};
use spacegate_tower::layers::{
    gateway::builder::SgGatewayLayerBuilder,
    http_route::builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
};
pub use spacegate_tower::SgBoxLayer;
pub use tardis::serde_json;
pub use tardis::serde_json::{Error as SerdeJsonError, Value as JsonValue};

pub use tower::BoxError;
#[cfg(feature = "cache")]
pub mod cache;
pub mod model;
pub mod plugins;

pub trait Plugin {
    type Error: std::error::Error + Send + Sync + 'static;
    type MakeLayer: MakeSgLayer + 'static;
    const CODE: &'static str;
    fn create(value: JsonValue) -> Result<Self::MakeLayer, Self::Error>;
}

pub trait MakeSgLayer {
    fn make_layer(&self) -> Result<SgBoxLayer, BoxError>;
    fn install_on_gateway(&self, gateway: SgGatewayLayerBuilder) -> Result<SgGatewayLayerBuilder, BoxError> {
        let layer = self.make_layer()?;
        Ok(gateway.http_plugin(layer))
    }
    fn install_on_backend(&self, backend: SgHttpBackendLayerBuilder) -> Result<SgHttpBackendLayerBuilder, BoxError> {
        let layer = self.make_layer()?;
        Ok(backend.plugin(layer))
    }
    fn install_on_route(&self, route: SgHttpRouteLayerBuilder) -> Result<SgHttpRouteLayerBuilder, BoxError> {
        let layer = self.make_layer()?;
        Ok(route.plugin(layer))
    }
    fn install_on_rule(&self, rule: SgHttpRouteRuleLayerBuilder) -> Result<SgHttpRouteRuleLayerBuilder, BoxError> {
        let layer = self.make_layer()?;
        Ok(rule.plugin(layer))
    }
}

type BoxCreateFn = Box<dyn Fn(JsonValue) -> Result<Box<dyn MakeSgLayer>, BoxError> + Send + Sync>;
#[derive(Default, Clone)]
pub struct SgPluginRepository {
    pub map: Arc<RwLock<HashMap<&'static str, BoxCreateFn>>>,
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
        #[cfg(feature = "decompression")]
        self.register::<plugins::decompression::DecompressionPlugin>()
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<P: Plugin>(&self) {
        let mut map = self.map.write().expect("SgPluginTypeMap register error");
        let create_fn = Box::new(move |value| P::create(value).map_err(BoxError::from).map(|x| Box::new(x) as Box<dyn MakeSgLayer>));
        map.insert(P::CODE, Box::new(create_fn));
    }

    pub fn register_custom<F, M, E>(&self, code: &'static str, f: F)
    where
        F: Fn(JsonValue) -> Result<M, E> + 'static + Send + Sync,
        M: MakeSgLayer + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let mut map = self.map.write().expect("SgPluginTypeMap register error");
        let create_fn = Box::new(move |value| f(value).map_err(BoxError::from).map(|x| Box::new(x) as Box<dyn MakeSgLayer>));
        map.insert(code, Box::new(create_fn));
    }

    pub fn create(&self, code: &str, value: JsonValue) -> Result<Box<dyn MakeSgLayer>, BoxError> {
        let map = self.map.read().expect("SgPluginTypeMap register error");
        if let Some(t) = map.get(code) {
            (t)(value)
        } else {
            Err(format!("[Sg.Plugin] unregistered sg plugin type {code}").into())
        }
    }

    pub fn create_layer(&self, code: &str, value: JsonValue) -> Result<SgBoxLayer, BoxError> {
        self.create(code, value)?.make_layer()
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
            type Error = $crate::SerdeJsonError;
            fn create(value: $crate::JsonValue) -> Result<Self::MakeLayer, Self::Error> {
                let filter: $filter_type = $crate::serde_json::from_value(value)?;
                Ok(filter)
            }
        }
    };
}

/// # Define Plugin Filter
///
/// use `def_filter_plugin` macro to define a filter plugin for an exsited struct which implemented [Filter](spacegate_tower::helper_layers::filter::Filter).
///
/// ```
/// # use serde::{Serialize, Deserialize};
/// # use hyper::{http::{StatusCode, header::AUTHORIZATION}, Response, Request};
/// # use spacegate_tower::{SgResponseExt, SgBody};
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
            type Error = $crate::SerdeJsonError;
            fn create(value: $crate::JsonValue) -> Result<Self::MakeLayer, Self::Error> {
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
