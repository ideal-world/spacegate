use crate::service::{config_format::ConfigFormat, Discovery};

use super::Fs;

impl<F: ConfigFormat + Send + Sync> Discovery for Fs<F> {
    async fn api_url(&self) -> Result<Option<String>, spacegate_model::BoxError> {
        self.retrieve_cached(|c| c.api_port.map(|p| format!("localhost:{}", p))).await
    }
}
