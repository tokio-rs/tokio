use super::File;
use std::future::Future;
use std::task::Poll;
use std::task::Context;
use std::fs::OpenOptions as StdOpenOptions;
use std::io;
use std::path::Path;
use std::pin::Pin;

/// Future returned by `File::open` and resolves to a `File` instance.
#[derive(Debug)]
pub struct OpenFuture<P> {
    options: StdOpenOptions,
    path: P,
}

impl<P> OpenFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(options: StdOpenOptions, path: P) -> Self {
        OpenFuture { options, path }
    }
}

impl<P> Unpin for OpenFuture<P> {} 

impl<P> Future for OpenFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Output = io::Result<File>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let std = ready!(crate::blocking_io(|| self.options.open(&self.path)))?;

        let file = File::from_std(std);
        Poll::Ready(Ok(file.into()))
    }
}
