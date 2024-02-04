use std::ffi::{OsStr, OsString};

use super::ConfigFormat;

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
    type Error = serde_json::Error;
    fn extension(&self) -> &OsStr {
        &self.extension
    }
    fn de<T: serde::de::DeserializeOwned>(&self, slice: &[u8]) -> Result<T, serde_json::Error> {
        serde_json::from_slice(slice)
    }
    fn ser<T: serde::Serialize>(&self, t: &T) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(t)
    }
}