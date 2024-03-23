use std::{collections::HashMap, sync::Arc};

pub use axum;
use axum::serve::Serve;
use axum::Router;
use tokio::sync::RwLock;
// pub struct AxumServerRepo {
//     inner: Arc<RwLock<HashMap<String, Serve>>>,
// }

pub struct AxumPluginRouter {
    router: axum::Router,
}

impl AxumPluginRouter {
    fn x(&self) {}
}

pub struct PluginRouter {
    inner: Arc<RwLock<HashMap<String, Router>>>,
}
