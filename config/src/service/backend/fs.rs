use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::service::config_format::ConfigFormat;

pub const GATEWAY_SUFFIX: &str = "gateway";
pub const ROUTES_SUFFIX: &str = "routes";
#[derive(Debug, Clone)]
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

    pub fn extract_gateway_name(&self, path: &Path) -> Option<String> {
        let ext = self.gateway_suffix().into_string().expect("invalid gateway suffix");
        path.file_name().and_then(OsStr::to_str).and_then(|f| {
            if f.ends_with(&ext) {
                Some(f.trim_end_matches(&ext).trim_end_matches('.').to_string())
            } else {
                None
            }
        })
    }
    pub fn extract_gateway_name_from_route_dir(&self, path: &Path) -> Option<String> {
        if path.extension()? == OsStr::from_bytes(ROUTES_SUFFIX.as_bytes()) {
            path.file_stem().and_then(OsStr::to_str).map(|f| f.to_string())
        } else {
            None
        }
    }
    pub fn extract_route_name(&self, path: &Path) -> Option<(String, String)> {
        let gateway_name = self.extract_gateway_name_from_route_dir(path.parent()?)?;
        if path.extension()? == self.format.extension() {
            let route_name = path.file_stem().and_then(OsStr::to_str).map(|f| f.to_string())?;
            Some((gateway_name, route_name))
        } else {
            None
        }
    }
}
