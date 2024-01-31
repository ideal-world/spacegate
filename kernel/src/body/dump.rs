use std::{
    pin::{pin, Pin},
    sync::Arc,
};

use futures_util::{ready, Future};
use http_body_util::{
    combinators::{BoxBody, Collect},
    BodyExt, Collected, Full,
};
use hyper::body::{Body, Bytes};
use tokio::sync::{Mutex, RwLock};

use crate::utils::never;

pub struct Dump {
    inner: DumpInnerState,
}

impl Dump {
    pub fn new(body: BoxBody<Bytes, hyper::Error>) -> Self {
        Self {
            inner: DumpInnerState::Collecting(Arc::new(Mutex::new(body.collect()))),
        }
    }
}

impl Clone for Dump {
    fn clone(&self) -> Self {
        Self {
            inner: match &self.inner {
                DumpInnerState::Collecting(collected) => DumpInnerState::Collecting(collected.clone()),
                DumpInnerState::Done { source, .. } => DumpInnerState::Done {
                    source: source.clone(),
                    copy: Full::new(source.clone()),
                },
            },
        }
    }
}

pub enum DumpInnerState {
    Collecting(Arc<Mutex<Collect<BoxBody<Bytes, hyper::Error>>>>),
    Done { source: Bytes, copy: Full<Bytes> },
}

impl Body for Dump {
    type Data = Bytes;

    type Error = hyper::Error;

    fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let inner = pin!(&mut self.inner);
        let inner_ref = inner.get_mut();
        loop {
            **inner_ref = match inner_ref {
                DumpInnerState::Collecting(ref mut collected) => {
                    let mut collected = ready!(pin!(collected.lock()).poll(cx));
                    let collected = ready!(pin!(&mut *collected).poll(cx));
                    match collected {
                        Ok(body) => {
                            let source = body.to_bytes();
                            let copy = Full::new(source.clone());
                            DumpInnerState::Done { source, copy }
                        }
                        Err(e) => {
                            return std::task::Poll::Ready(Some(Err(e)));
                        }
                    }
                }
                DumpInnerState::Done { ref mut copy, .. } => {
                    return pin!(copy).poll_frame(cx).map_err(never);
                }
            }
        }
    }
}
