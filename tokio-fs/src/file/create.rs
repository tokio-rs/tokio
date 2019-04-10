use super::File;
use crate::blocking_pool::{blocking, Blocking};
use futures::{future, try_ready, Async, Future, Poll};
use std::fs::File as StdFile;
use std::io;
use std::path::Path;

/// Future returned by `File::create` and resolves to a `File` instance.
#[derive(Debug)]
pub struct CreateFuture<P>(Mode<P>);

#[derive(Debug)]
enum Mode<P> {
    Native { path: P },
    Fallback(Blocking<StdFile, io::Error>),
}

impl<P> CreateFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(path: P) -> Self {
        CreateFuture(if tokio_threadpool::entered() {
            Mode::Native { path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || StdFile::create(&path))))
        })
    }
}

impl<P> Future for CreateFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = File;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let std = match &mut self.0 {
            Mode::Native { path } => try_ready!(crate::blocking_io(|| StdFile::create(&path))),
            Mode::Fallback(job) => try_ready!(job.poll()),
        };
        Ok(Async::Ready(File::from_std(std)))
    }
}
