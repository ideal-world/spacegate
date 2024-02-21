use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::config_format::ConfigFormat;

pub const GATEWAY_SUFFIX: &str = "gateway";
pub const ROUTES_SUFFIX: &str = "routes";

pub struct Fs<F> {
    pub dir: Arc<Path>,
    pub format: F,
}

impl<F> Fs<F>
where
    F: ConfigFormat,
{
    pub fn new<P: AsRef<Path>>(dir: P, format: F) -> Self {
        Self {
            dir: Arc::from(dir.as_ref().to_owned()),
            format,
        }
    }

    pub fn gateway_suffix(&self) -> OsString {
        let mut ext = OsString::from(GATEWAY_SUFFIX);
        ext.push(OsStr::from_bytes(b"."));
        ext.push(self.format.extension());
        ext
    }

    pub fn gateway_path(&self, name: &str) -> PathBuf {
        self.dir.join(name).with_extension(self.gateway_suffix())
    }

    pub fn routes_dir(&self, gateway_name: &str) -> PathBuf {
        self.dir.join(gateway_name).with_extension(ROUTES_SUFFIX)
    }

    pub fn route_path(&self, gateway_name: &str, route_name: &str) -> PathBuf {
        self.routes_dir(gateway_name).join(route_name).with_extension(self.format.extension())
    }
}
