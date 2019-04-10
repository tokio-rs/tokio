use super::blocking_io;
use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs::{self, Metadata};
use std::io;
use std::path::Path;
use tokio_threadpool;

/// Queries the file system metadata for a path.
pub fn metadata<P>(path: P) -> MetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    MetadataFuture::new(path)
}

/// Future returned by `metadata`.
#[derive(Debug)]
pub struct MetadataFuture<P>(Mode<P>)
where
    P: AsRef<Path> + Send + 'static;

#[derive(Debug)]
enum Mode<P>
where
    P: AsRef<Path> + Send + 'static,
{
    Native { path: P },
    Fallback(Blocking<Metadata, io::Error>),
}

impl<P> MetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(path: P) -> Self {
        MetadataFuture(if tokio_threadpool::entered() {
            Mode::Native { path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::metadata(&path))))
        })
    }
}

impl<P> Future for MetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = Metadata;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { path } => blocking_io(|| fs::metadata(path)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
