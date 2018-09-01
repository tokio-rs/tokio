use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use futures::{Future, Poll};

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
pub struct ReadLinkFuture<P>
where
    P: AsRef<Path>
{
    path: P,
}

impl<P> ReadLinkFuture<P>
where
    P: AsRef<Path>
{
    fn new(path: P) -> ReadLinkFuture<P> {
        ReadLinkFuture {
            path: path,
        }
    }
}

impl<P> Future for ReadLinkFuture<P>
where
    P: AsRef<Path>
{
    type Item = PathBuf;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::read_link(&self.path) )
    }
}
