use super::File;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Future returned by `File::seek`.
#[derive(Debug)]
pub struct SeekFuture {
    inner: Option<File>,
    pos: io::SeekFrom,
}

impl SeekFuture {
    pub(crate) fn new(file: File, pos: io::SeekFrom) -> Self {
        Self {
            pos,
            inner: Some(file),
        }
    }
}

impl Future for SeekFuture {
    type Output = io::Result<(File, u64)>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner_self = Pin::get_mut(self);
        let pos = ready!(inner_self
            .inner
            .as_mut()
            .expect("Cannot poll `SeekFuture` after it resolves")
            .poll_seek(inner_self.pos))?;
        let inner = inner_self.inner.take().unwrap();
        Poll::Ready(Ok((inner, pos).into()))
    }
}
