use super::File;
use std::fs::File as StdFile;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Future returned by `File::create` and resolves to a `File` instance.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[derive(Debug)]
pub struct CreateFuture<P> {
    path: P,
}

impl<P> CreateFuture<P>
where
    P: AsRef<Path> + Send + Unpin + 'static,
{
    pub(crate) fn new(path: P) -> Self {
        CreateFuture { path }
    }
}

impl<P> Future for CreateFuture<P>
where
    P: AsRef<Path> + Send + Unpin + 'static,
{
    type Output = io::Result<File>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let std = ready!(crate::blocking_io(|| StdFile::create(&self.path)))?;

        let file = File::from_std(std);
        Poll::Ready(Ok(file.into()))
    }
}
