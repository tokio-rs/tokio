use std::fs;
use std::io;
use std::path::Path;

use futures::{Future, Poll};

/// Creates a new, empty directory at the provided path
///
/// This is an async version of [`std::fs::create_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir.html
pub fn create_dir<P: AsRef<Path>>(path: P) -> CreateDirFuture<P> {
    CreateDirFuture::new(path)
}

/// Future returned by `create_dir`.
#[derive(Debug)]
pub struct CreateDirFuture<P>
where
    P: AsRef<Path>
{
    path: P,
}

impl<P> CreateDirFuture<P>
where
    P: AsRef<Path>
{
    fn new(path: P) -> CreateDirFuture<P> {
        CreateDirFuture {
            path: path,
        }
    }
}

impl<P> Future for CreateDirFuture<P>
where
    P: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::create_dir(&self.path) )
    }
}
