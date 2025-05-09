// a read only stream reader with some side effect.
pub mod observer;
use crate::BoxError;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::body::{Body, Bytes};

use crate::utils::never;

/// # Clone
/// Always clone after ensure the body is dumped
#[derive(Debug)]
pub struct SgBody {
    pub(crate) body: BoxBody<Bytes, BoxError>,
    pub(crate) dump: Option<Bytes>,
}

impl Default for SgBody {
    fn default() -> Self {
        Self::empty()
    }
}

impl Body for SgBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let mut pinned = std::pin::pin!(&mut self.body);
        pinned.as_mut().poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.body.size_hint()
    }
}

impl SgBody {
    pub fn new<E>(body: impl Body<Data = Bytes, Error = E> + Send + Sync + 'static) -> Self
    where
        E: Into<BoxError> + 'static,
    {
        Self {
            body: BoxBody::new(body.map_err(E::into)),
            dump: None,
        }
    }
    pub fn empty() -> Self {
        Self {
            body: BoxBody::new(Empty::new().map_err(never)),
            dump: None,
        }
    }
    pub fn full(data: impl Into<Bytes>) -> Self {
        let bytes = data.into();
        Self {
            body: BoxBody::new(Full::new(bytes.clone()).map_err(never)),
            dump: Some(bytes),
        }
    }
    pub fn is_dumped(&self) -> bool {
        self.dump.is_some()
    }
    /// # Errors
    /// fail to collect body chunks
    pub async fn dump(self) -> Result<Self, BoxError> {
        let bytes = self.body.collect().await?.to_bytes();
        Ok(Self {
            body: BoxBody::new(Full::new(bytes.clone()).map_err(never)),
            dump: Some(bytes),
        })
    }
    pub fn dump_clone(&self) -> Option<Self> {
        self.dump.as_ref().map(|bytes| Self {
            body: BoxBody::new(Full::new(bytes.clone()).map_err(never)),
            dump: Some(bytes.clone()),
        })
    }
    pub fn get_dumped(&self) -> Option<&Bytes> {
        self.dump.as_ref()
    }
}

impl Clone for SgBody {
    fn clone(&self) -> Self {
        if let Some(dump) = self.dump_clone() {
            dump
        } else {
            panic!("SgBody can't be cloned before dump")
        }
    }
}
