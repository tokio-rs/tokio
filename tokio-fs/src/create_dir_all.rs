use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs;
use std::io;
use std::path::Path;

/// Recursively create a directory and all of its parent components if they
/// are missing.
///
/// This is an async version of [`std::fs::create_dir_all`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir_all.html
pub fn create_dir_all<P>(path: P) -> CreateDirAllFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    CreateDirAllFuture::new(path)
}

/// Future returned by `create_dir_all`.
#[derive(Debug)]
pub struct CreateDirAllFuture<P>(Mode<P>)
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

impl<P> CreateDirAllFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    fn new(path: P) -> CreateDirAllFuture<P> {
        CreateDirAllFuture(if tokio_threadpool::entered() {
            Mode::Native { path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::create_dir_all(&path))))
        })
    }
}

impl<P> Future for CreateDirAllFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { path } => crate::blocking_io(|| fs::create_dir_all(path)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
