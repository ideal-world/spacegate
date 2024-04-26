use spacegate_model::{BackendHost, BoxResult};

use crate::service::Discovery;

use super::K8s;

impl Discovery for K8s {
    async fn api_url(&self) -> BoxResult<Option<String>> {
        todo!()
    }

    async fn backends(&self) -> BoxResult<Vec<BackendHost>> {
        todo!();
        Ok(vec![])
    }
}
