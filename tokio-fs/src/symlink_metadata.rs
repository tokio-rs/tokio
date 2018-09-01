use super::blocking_io;

use futures::{Future, Poll};

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
pub struct SymlinkMetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    path: P,
}

impl<P> SymlinkMetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(path: P) -> Self {
        Self { path }
    }
}

impl<P> Future for SymlinkMetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = Metadata;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        blocking_io(|| fs::symlink_metadata(&self.path))
    }
}
