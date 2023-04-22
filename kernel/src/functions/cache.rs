use std::collections::HashMap;

use tardis::{
    basic::{error::TardisError, result::TardisResult},
    cache::cache_client::TardisCacheClient,
};

static mut CACHES: Option<HashMap<String, TardisCacheClient>> = None;

pub async fn init(name: &str, url: &str) -> TardisResult<()> {
    let cache = TardisCacheClient::init(url).await?;
    unsafe {
        if CACHES.is_none() {
            CACHES = Some(HashMap::new());
        }
        CACHES.as_mut().unwrap().insert(name.to_string(), cache);
    }
    Ok(())
}

pub async fn remove(name: &str) -> TardisResult<()> {
    unsafe {
        if CACHES.is_none() {
            CACHES = Some(HashMap::new());
        }
        CACHES.as_mut().unwrap().remove(name);
    }
    Ok(())
}

pub fn get(name: &str) -> TardisResult<&'static TardisCacheClient> {
    unsafe {
        if let Some(cache) = CACHES.as_ref().unwrap().get(name) {
            Ok(cache)
        } else {
            Err(TardisError::bad_request(&format!("[SG.server] Get cache {name} failed"), ""))
        }
    }
}
