use spacegate_model::BackendHost;

use crate::service::{config_format::ConfigFormat, Discovery};

use super::Fs;

impl<F: ConfigFormat + Send + Sync> Discovery for Fs<F> {
    async fn api_url(&self) -> Result<Option<String>, spacegate_model::BoxError> {
        self.retrieve_cached(|c| c.api_port.map(|p| format!("localhost:{}", p))).await
    }
    #[cfg(target_os = "linux")]
    async fn backends(&self) -> Result<Vec<BackendHost>, spacegate_model::BoxError> {
        // read /var/www
        let mut dir = tokio::fs::read_dir("/var/www").await?;
        let mut collector = vec![];
        while let Ok(Some(entry)) = dir.next_entry().await {
            if entry.path().is_dir() {
                if let Some(path) = entry.path().to_str() {
                    collector.push(BackendHost::File { path: path.to_string() })
                }
            }
        }
        Ok(collector)
    }
}
