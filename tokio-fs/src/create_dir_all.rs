use std::fs;
use std::io;
use std::path::Path;

use futures::{Future, Poll};

/// Recursively create a directory and all of its parent components if they
/// are missing.
///
/// This is an async version of [`std::fs::create_dir_all`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir_all.html
pub fn create_dir_all<P: AsRef<Path>>(path: P) -> CreateDirAllFuture<P> {
    CreateDirAllFuture::new(path)
}

/// Future returned by `create_dir_all`.
#[derive(Debug)]
pub struct CreateDirAllFuture<P>
where
    P: AsRef<Path>
{
    path: P,
}

impl<P> CreateDirAllFuture<P>
where
    P: AsRef<Path>
{
    fn new(path: P) -> CreateDirAllFuture<P> {
        CreateDirAllFuture {
            path: path,
        }
    }
}

impl<P> Future for CreateDirAllFuture<P>
where
    P: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::create_dir_all(&self.path) )
    }
}
