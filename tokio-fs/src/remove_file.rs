use std::fs;
use std::io;
use std::path::Path;

use futures::{Future, Poll};

/// Removes a file from the filesystem.
///
/// Note that there is no
/// guarantee that the file is immediately deleted (e.g. depending on
/// platform, other open file descriptors may prevent immediate removal).
///
/// This is an async version of [`std::fs::remove_file`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.remove_file.html
pub fn remove_file<P: AsRef<Path>>(path: P) -> RemoveFileFuture<P> {
    RemoveFileFuture::new(path)
}

/// Future returned by `remove_file`.
#[derive(Debug)]
pub struct RemoveFileFuture<P>
where
    P: AsRef<Path>
{
    path: P,
}

impl<P> RemoveFileFuture<P>
where
    P: AsRef<Path>
{
    fn new(path: P) -> RemoveFileFuture<P> {
        RemoveFileFuture {
            path: path,
        }
    }
}

impl<P> Future for RemoveFileFuture<P>
where
    P: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::remove_file(&self.path) )
    }
}
