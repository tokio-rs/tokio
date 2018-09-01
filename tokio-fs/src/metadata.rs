use super::blocking_io;

use futures::{Future, Poll};

use std::fs::{self, Metadata};
use std::io;
use std::path::Path;

/// Queries the file system metadata for a path.
pub fn metadata<P>(path: P) -> MetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    MetadataFuture::new(path)
}

/// Future returned by `metadata`.
#[derive(Debug)]
pub struct MetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    path: P,
}

impl<P> MetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(path: P) -> Self {
        Self { path }
    }
}

impl<P> Future for MetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = Metadata;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        blocking_io(|| fs::metadata(&self.path))
    }
}
