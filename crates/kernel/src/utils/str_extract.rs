use crate::{extension::OriginalIpAddr, SgRequest};

/// #
/// ## headers:
/// `parts/header/<header-name>`
/// ## uri:
/// `parts/uri`
/// ### host
/// `parts/uri/host`
/// ### port
/// `parts/uri/port`
/// ### path
/// `parts/uri/path`
/// ### query
/// `parts/uri/query`
/// #### query value
/// `parts/uri/query/<key>`
/// ### scheme
/// `parts/uri/scheme`
/// ## extensions
/// `ext/<extension-name>`
#[derive(Debug)]
pub enum StrExtractorKind<'a> {
    Parts(StrExtractorPartsKind<'a>),
    Extension(&'a str),
}

#[derive(Debug)]
pub enum StrExtractorPartsKind<'a> {
    Method,
    Uri,
    Header(&'a str),
}

impl<'a> StrExtractorKind<'a> {
    pub fn try_from_str(str: &'a str) -> Option<Self> {
        let kind = match str.split_once('/')? {
            ("parts", "uri") => StrExtractorKind::Parts(StrExtractorPartsKind::Uri),
            ("parts", "method") => StrExtractorKind::Parts(StrExtractorPartsKind::Method),
            ("parts", other_part) => match other_part.split_once("/")? {
                ("header", header_name) => StrExtractorKind::Parts(StrExtractorPartsKind::Header(header_name)),
                _ => return None,
            },
            ("ext", parts) => StrExtractorKind::Extension(parts),
            _ => return None,
        };
        Some(kind)
    }
}

impl<'a> StrExtractorKind<'a> {
    pub fn extract_to_string(&self, request: &SgRequest) -> Option<String> {
        match self {
            StrExtractorKind::Parts(str_extractor_parts_kind) => match str_extractor_parts_kind {
                StrExtractorPartsKind::Method => Some(request.method().to_string()),
                StrExtractorPartsKind::Uri => Some(request.uri().to_string()),
                StrExtractorPartsKind::Header(header_name) => {
                    let header_name = header_name.to_lowercase();
                    Some(request.headers().get(header_name.as_str())?.to_str().ok()?.to_string())
                }
            },
            StrExtractorKind::Extension(e) => match *e {
                "original_ip_addr" => Some(request.extensions().get::<OriginalIpAddr>()?.to_string()),
                _ => None,
            },
        }
    }
}
