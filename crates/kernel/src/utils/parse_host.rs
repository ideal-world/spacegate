#[derive(Debug)]
pub struct HostAndPort<'a> {
    pub host: &'a [u8],
    pub port: Option<&'a [u8]>,
}

impl<'a> HostAndPort<'a> {
    #[inline]
    pub fn host_end_with(&self, suffix: &[u8]) -> bool {
        self.host.ends_with(suffix)
    }
    #[allow(clippy::indexing_slicing)]
    pub fn from_header(host: &'a hyper::http::HeaderValue) -> Self {
        Self::from_bytes(host.as_bytes())
    }
    #[allow(clippy::indexing_slicing)]
    pub fn from_bytes(host: &'a [u8]) -> Self {
        let bytes = host;
        let mut comma_token_pos = None;

        for (idx, byte) in bytes.iter().enumerate().rev() {
            if *byte == b':' {
                comma_token_pos = Some(idx);
                break;
            } else if !byte.is_ascii_digit() {
                break;
            }
        }
        if let Some(comma_token_pos) = comma_token_pos {
            let host = &bytes[..comma_token_pos];
            let port = if comma_token_pos == bytes.len() - 1 {
                None
            } else {
                Some(&bytes[comma_token_pos + 1..])
            };
            HostAndPort { host, port }
        } else {
            HostAndPort { host: bytes, port: None }
        }
    }
}
