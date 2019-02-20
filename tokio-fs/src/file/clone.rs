use super::File;

use futures::{Future, Poll};

use std::io;

/// Future returned by `File::try_clone`.
///
/// Clones a file handle into two file handles.
///
/// # Panics
///
/// Will panic if polled after returning an item or error.
#[derive(Debug)]
pub struct CloneFuture {
    file: Option<File>,
}

impl CloneFuture {
    pub(crate) fn new(file: File) -> Self {
        Self { file: Some(file) }
    }
}

impl Future for CloneFuture {
    type Item = (File, File);
    type Error = (File, io::Error);

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.file
            .as_mut()
            .expect("Cannot poll `CloneFuture` after it resolves")
            .poll_try_clone()
            .map(|inner| inner.map(|cloned| (self.file.take().unwrap(), cloned)))
            .map_err(|err| (self.file.take().unwrap(), err))
    }
}
