use std::sync::Arc;

use tokio::sync::RwLock;

use crate::Config;

/// In-memory Config Backend
#[derive(Debug, Clone)]
pub struct Memory {
    pub config: Arc<RwLock<Config>>,
}

impl Memory {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }
}
