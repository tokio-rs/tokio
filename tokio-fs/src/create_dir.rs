use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs;
use std::io;
use std::path::Path;

/// Creates a new, empty directory at the provided path
///
/// This is an async version of [`std::fs::create_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir.html
pub fn create_dir<P>(path: P) -> CreateDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    CreateDirFuture::new(path)
}

/// Future returned by `create_dir`.
#[derive(Debug)]
pub struct CreateDirFuture<P>(Mode<P>)
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

impl<P> CreateDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    fn new(path: P) -> CreateDirFuture<P> {
        CreateDirFuture(if tokio_threadpool::entered() {
            Mode::Native { path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::create_dir(&path))))
        })
    }
}

impl<P> Future for CreateDirFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { path } => crate::blocking_io(|| fs::create_dir(path)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
