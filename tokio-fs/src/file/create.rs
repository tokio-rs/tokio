use super::File;
use std::future::Future;
use std::task::Poll;
use std::task::Context;
use std::fs::File as StdFile;
use std::io;
use std::path::Path;
use std::pin::Pin;

/// Future returned by `File::create` and resolves to a `File` instance.
#[derive(Debug)]
pub struct CreateFuture<P> {
    path: P,
}

impl<P> CreateFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(path: P) -> Self {
        CreateFuture { path }
    }
}

impl<P> Unpin for CreateFuture<P> {} 

impl<P> Future for CreateFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Output = io::Result<File>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let std = ready!(crate::blocking_io(|| StdFile::create(&self.path)))?;

        let file = File::from_std(std);
        Poll::Ready(Ok(file.into()))
    }
}
