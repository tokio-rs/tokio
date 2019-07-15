use super::blocking_io;
use std::fs::{self, Metadata};
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

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
#[must_use = "futures do nothing unless you `.await` or poll them"]
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
    type Output = io::Result<Metadata>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        blocking_io(|| fs::symlink_metadata(&self.path))
    }
}
