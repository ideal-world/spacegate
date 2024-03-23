use std::{collections::HashMap, sync::Arc};

pub use axum;
use axum::Router;
use tokio::sync::RwLock;
use axum::serve::Serve;
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
