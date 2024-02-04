use std::ffi::{OsStr, OsString};
pub mod json;
// pub mod yaml;
// pub mod toml;
pub trait ConfigFormat {
    type Error;
    fn extension(&self) -> &OsStr;
    fn de<T: serde::de::DeserializeOwned>(&self, slice: &[u8]) -> Result<T, Self::Error>;
    fn ser<T: serde::Serialize>(&self, t: &T) -> Result<Vec<u8>, Self::Error>;
}


pub use json::Json;