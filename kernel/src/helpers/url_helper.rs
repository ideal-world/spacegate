use std::str::FromStr;

use http::Uri;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    url::Url,
};

pub(crate) trait UrlToUri {
    fn to_uri(&self) -> TardisResult<Uri>;
}

impl UrlToUri for Url {
    fn to_uri(&self) -> TardisResult<Uri> {
        Uri::from_str(self.as_str()).map_err(|e| TardisError::format_error(&format!("[SG.helper] Url to Uri error :{}", e), ""))
    }
}
