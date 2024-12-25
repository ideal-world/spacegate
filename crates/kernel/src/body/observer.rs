use std::task::Poll;

use crate::BoxError;
use hyper::body::{Body, Bytes, Frame};

use super::SgBody;

pub trait State: Sized + Send + Sync + 'static {
    fn update_bytes(&mut self, data: &Bytes);
    fn finish(self) {}
    fn error(self, _e: &BoxError) {}
}

pin_project_lite::pin_project! {
    pub struct Observer<S> {
        state: Option<S>,
        #[pin]
        inner: SgBody,
    }
}

impl<S: State> Observer<S> {
    pub fn new(state: S, inner: SgBody) -> Self {
        Self { state: Some(state), inner }
    }
    pub fn to_sg_body(self) -> SgBody {
        SgBody::new_boxed_error(self)
    }
}
impl<S> Body for Observer<S>
where
    S: State,
{
    type Data = Bytes;
    type Error = BoxError;
    fn poll_frame(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let poll_result = this.inner.poll_frame(cx);
        if let Poll::Ready(ref ready) = poll_result {
            match ready {
                Some(Ok(ref frame)) => {
                    if let Some(data) = frame.data_ref() {
                        if let Some(s) = this.state.as_mut() {
                            s.update_bytes(data)
                        }
                    }
                }
                Some(Err(ref e)) => {
                    if let Some(s) = this.state.take() {
                        s.error(e)
                    }
                }
                None => {
                    if let Some(s) = this.state.take() {
                        s.finish()
                    }
                }
            }
        }
        poll_result
    }
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }
}
