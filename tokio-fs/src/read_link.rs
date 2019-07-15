use std::fs;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Reads a symbolic link, returning the file that the link points to.
///
/// This is an async version of [`std::fs::read_link`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.read_link.html
pub fn read_link<P: AsRef<Path>>(path: P) -> ReadLinkFuture<P> {
    ReadLinkFuture::new(path)
}

/// Future returned by `read_link`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ReadLinkFuture<P>
where
    P: AsRef<Path>,
{
    path: P,
}

impl<P> ReadLinkFuture<P>
where
    P: AsRef<Path>,
{
    fn new(path: P) -> ReadLinkFuture<P> {
        ReadLinkFuture { path: path }
    }
}

impl<P> Future for ReadLinkFuture<P>
where
    P: AsRef<Path>,
{
    type Output = io::Result<PathBuf>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        crate::blocking_io(|| fs::read_link(&self.path))
    }
}
