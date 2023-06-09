use std::collections::HashMap;

use tardis::{
    basic::{error::TardisError, result::TardisResult},
    cache::cache_client::TardisCacheClient,
};

static mut CACHE_CLIENTS: Option<HashMap<String, TardisCacheClient>> = None;

pub async fn init(name: &str, url: &str) -> TardisResult<()> {
    let cache = TardisCacheClient::init(url).await?;
    unsafe {
        if CACHE_CLIENTS.is_none() {
            CACHE_CLIENTS = Some(HashMap::new());
        }
        CACHE_CLIENTS.as_mut().expect("Unreachable code").insert(name.to_string(), cache);
    }
    Ok(())
}

pub async fn remove(name: &str) -> TardisResult<()> {
    unsafe {
        if CACHE_CLIENTS.is_none() {
            CACHE_CLIENTS = Some(HashMap::new());
        }
        CACHE_CLIENTS.as_mut().expect("Unreachable code").remove(name);
    }
    Ok(())
}

pub fn get(name: &str) -> TardisResult<&'static TardisCacheClient> {
    unsafe {
        if let Some(client) = CACHE_CLIENTS.as_ref().ok_or_else(|| TardisError::bad_request("[SG.server] Get client failed", ""))?.get(name) {
            Ok(client)
        } else {
            Err(TardisError::bad_request(&format!("[SG.server] Get client {name} failed"), ""))
        }
    }
}
