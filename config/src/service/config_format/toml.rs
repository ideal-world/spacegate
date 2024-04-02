use std::ffi::{OsStr, OsString};

use crate::BoxError;

use super::ConfigFormat;
#[derive(Debug, Clone)]
pub struct Toml {
    pub extension: OsString,
}

impl Default for Toml {
    fn default() -> Self {
        Self {
            extension: OsString::from("toml"),
        }
    }
}

impl ConfigFormat for Toml {
    fn extension(&self) -> &OsStr {
        &self.extension
    }
    fn de<T: serde::de::DeserializeOwned>(&self, slice: &[u8]) -> Result<T, BoxError> {
        Ok(toml::from_str(&String::from_utf8_lossy(slice))?)
    }
    fn ser<T: serde::Serialize>(&self, t: &T) -> Result<Vec<u8>, BoxError> {
        Ok(toml::to_string_pretty(t)?.into())
    }
}
