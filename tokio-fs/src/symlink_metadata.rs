use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs::{self, Metadata};
use std::io;
use std::path::Path;

/// Queries the file system metadata for a path.
///
/// This is an async version of [`std::fs::symlink_metadata`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.symlink_metadata.html
pub fn symlink_metadata<P>(path: P) -> SymlinkMetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    SymlinkMetadataFuture::new(path)
}

/// Future returned by `symlink_metadata`.
#[derive(Debug)]
pub struct SymlinkMetadataFuture<P>(Mode<P>)
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

impl<P> SymlinkMetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(path: P) -> Self {
        SymlinkMetadataFuture(if tokio_threadpool::entered() {
            Mode::Native { path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::symlink_metadata(path))))
        })
    }
}

impl<P> Future for SymlinkMetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = Metadata;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { path } => crate::blocking_io(|| fs::symlink_metadata(path)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
