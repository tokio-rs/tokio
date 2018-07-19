use std::fs;
use std::io;
use std::path::Path;

use futures::{Future, Poll};

/// Rename a file or directory to a new name, replacing the original file if
/// `to` already exists.
///
/// This will not work if the new name is on a different mount point.
///
/// This is an async version of [`std::fs::rename`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.rename.html
pub fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> RenameFuture<P, Q> {
    RenameFuture::new(from, to)
}

/// Future returned by `rename`.
#[derive(Debug)]
pub struct RenameFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    from: P,
    to: Q,
}

impl<P, Q> RenameFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    fn new(from: P, to: Q) -> RenameFuture<P, Q> {
        RenameFuture {
            from: from,
            to: to,
        }
    }
}

impl<P, Q> Future for RenameFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::rename(&self.from, &self.to) )
    }
}
