use spacegate_model::BackendHost;

use crate::service::{config_format::ConfigFormat, Discovery, Instance};

use super::Fs;
pub struct LocalGateway {
    uri: String,
}

impl LocalGateway {
    pub fn new(port: u16) -> Self {
        Self {
            uri: format!("localhost:{}", port),
        }
    }
}

impl Instance for LocalGateway {
    fn api_url(&self) -> &str {
        &self.uri
    }
    fn id(&self) -> &str {
        "local"
    }
}
impl<F: ConfigFormat + Send + Sync + 'static> Discovery for Fs<F> {
    async fn instances(&self) -> Result<Vec<impl Instance>, spacegate_model::BoxError> {
        self.retrieve_cached(|c| c.api_port.map(LocalGateway::new).into_iter().collect()).await
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
