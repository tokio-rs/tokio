use super::File;
use crate::blocking_pool::{blocking, Blocking};
use futures::{future, try_ready, Async, Future, Poll};
use std::fs::OpenOptions as StdOpenOptions;
use std::io;
use std::path::Path;

/// Future returned by `File::open` and resolves to a `File` instance.
#[derive(Debug)]
pub struct OpenFuture<P>(Mode<P>);

#[derive(Debug)]
enum Mode<P> {
    Native { options: StdOpenOptions, path: P },
    Fallback(Blocking<StdFile, io::Error>),
}

impl<P> OpenFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(options: StdOpenOptions, path: P) -> Self {
        OpenFuture(if tokio_threadpool::entered() {
            Mode::Native { options, path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || options.open(path))))
        })
    }
}

impl<P> Future for OpenFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = File;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let std = match &mut self.0 {
            Mode::Native { options, path } => {
                try_ready!(crate::blocking_io(|| options.open(&path)))
            }
            Mode::Fallback(job) => try_ready!(job.poll()),
        };
        Ok(Async::Ready(File::from_std(std)))
    }
}
