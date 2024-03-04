use std::ffi::OsStr;
pub mod json;
// pub mod yaml;
// pub mod toml;
pub trait ConfigFormat {
    fn extension(&self) -> &OsStr;
    fn de<T: serde::de::DeserializeOwned>(&self, slice: &[u8]) -> Result<T, BoxError>;
    fn ser<T: serde::Serialize>(&self, t: &T) -> Result<Vec<u8>, BoxError>;
}

pub use json::Json;

use crate::BoxError;
