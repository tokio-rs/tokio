use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs;
use std::io;
use std::path::Path;

/// Removes an existing, empty directory.
///
/// This is an async version of [`std::fs::remove_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.remove_dir.html
pub fn remove_dir<P>(path: P) -> RemoveDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    RemoveDirFuture::new(path)
}

/// Future returned by `remove_dir`.
#[derive(Debug)]
pub struct RemoveDirFuture<P>(Mode<P>)
where
    P: AsRef<Path> + Send + 'static;

#[derive(Debug)]
enum Mode<P>
where
    P: AsRef<Path> + Send + 'static,
{
    Native { path: P },
    Fallback(Blocking<(), io::Error>),
}

impl<P> RemoveDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    fn new(path: P) -> RemoveDirFuture<P> {
        RemoveDirFuture(if tokio_threadpool::entered() {
            Mode::Native { path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::remove_dir(&path))))
        })
    }
}

impl<P> Future for RemoveDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { path } => crate::blocking_io(|| fs::remove_dir(path)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
