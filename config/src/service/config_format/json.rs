use std::ffi::{OsStr, OsString};

use crate::BoxError;

use super::ConfigFormat;
#[derive(Debug, Clone)]
pub struct Json {
    pub extension: OsString,
}

impl Default for Json {
    fn default() -> Self {
        Self {
            extension: OsString::from("json"),
        }
    }
}

impl ConfigFormat for Json {
    fn extension(&self) -> &OsStr {
        &self.extension
    }
    fn de<T: serde::de::DeserializeOwned>(&self, slice: &[u8]) -> Result<T, BoxError> {
        Ok(serde_json::from_slice(slice)?)
    }
    fn ser<T: serde::Serialize>(&self, t: &T) -> Result<Vec<u8>, BoxError> {
        Ok(serde_json::to_vec(t)?)
    }
}
