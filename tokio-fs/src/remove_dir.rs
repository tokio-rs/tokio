use std::fs;
use std::io;
use std::path::Path;

use futures::{Future, Poll};

/// Removes an existing, empty directory.
///
/// This is an async version of [`std::fs::remove_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.remove_dir.html
pub fn remove_dir<P: AsRef<Path>>(path: P) -> RemoveDirFuture<P> {
    RemoveDirFuture::new(path)
}

/// Future returned by `remove_dir`.
#[derive(Debug)]
pub struct RemoveDirFuture<P>
where
    P: AsRef<Path>
{
    path: P,
}

impl<P> RemoveDirFuture<P>
where
    P: AsRef<Path>
{
    fn new(path: P) -> RemoveDirFuture<P> {
        RemoveDirFuture {
            path: path,
        }
    }
}

impl<P> Future for RemoveDirFuture<P>
where
    P: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::remove_dir(&self.path) )
    }
}
