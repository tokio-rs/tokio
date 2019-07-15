use super::blocking_io;
use std::fs::{self, Metadata};
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Queries the file system metadata for a path.
pub fn metadata<P>(path: P) -> MetadataFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    MetadataFuture::new(path)
}

/// Future returned by `metadata`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
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
    type Output = io::Result<Metadata>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        blocking_io(|| fs::metadata(&self.path))
    }
}
