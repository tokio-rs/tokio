use super::File;

use futures_core::ready;
use std::fs::OpenOptions as StdOpenOptions;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Future returned by `File::open` and resolves to a `File` instance.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct OpenFuture<P: Unpin> {
    options: StdOpenOptions,
    path: P,
}

impl<P> OpenFuture<P>
where
    P: AsRef<Path> + Send + Unpin + 'static,
{
    pub(crate) fn new(options: StdOpenOptions, path: P) -> Self {
        OpenFuture { options, path }
    }
}

impl<P> Future for OpenFuture<P>
where
    P: AsRef<Path> + Send + Unpin + 'static,
{
    type Output = io::Result<File>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let std = ready!(crate::blocking_io(|| self.options.open(&self.path)))?;

        let file = File::from_std(std);
        Poll::Ready(Ok(file.into()))
    }
}
