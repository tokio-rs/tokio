use super::File;

use futures::{Future, Poll};

use std::io;

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
    type Item = (File, u64);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let pos = try_ready!(
            self.inner
                .as_mut()
                .expect("Cannot poll `SeekFuture` after it resolves")
                .poll_seek(self.pos)
        );
        let inner = self.inner.take().unwrap();
        Ok((inner, pos).into())
    }
}
