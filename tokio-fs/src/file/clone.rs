use super::File;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Future returned by `File::try_clone`.
///
/// Clones a file handle into two file handles.
///
/// # Panics
///
/// Will panic if polled after returning an item or error.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct CloneFuture {
    file: Option<File>,
}

impl CloneFuture {
    pub(crate) fn new(file: File) -> Self {
        Self { file: Some(file) }
    }
}

impl Future for CloneFuture {
    type Output = Result<(File, File), (File, io::Error)>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner_self = Pin::get_mut(self);
        inner_self
            .file
            .as_mut()
            .expect("Cannot poll `CloneFuture` after it resolves")
            .poll_try_clone()
            .map(|inner| inner.map(|cloned| (inner_self.file.take().unwrap(), cloned)))
            .map_err(|err| (inner_self.file.take().unwrap(), err))
    }
}
