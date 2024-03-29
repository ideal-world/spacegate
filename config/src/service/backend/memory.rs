use std::sync::Arc;

use crate::Config;

/// In-memory Config Backend
#[derive(Debug, Clone)]
pub struct Memory {
    pub config: Arc<Config>,
}

impl Memory {
    pub fn new(config: Config) -> Self {
        Self { config: Arc::new(config) }
    }
}
