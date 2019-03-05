use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs;
use std::io;
use std::path::Path;

/// Removes a file from the filesystem.
///
/// Note that there is no
/// guarantee that the file is immediately deleted (e.g. depending on
/// platform, other open file descriptors may prevent immediate removal).
///
/// This is an async version of [`std::fs::remove_file`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.remove_file.html
pub fn remove_file<P>(path: P) -> RemoveFileFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    RemoveFileFuture::new(path)
}

/// Future returned by `remove_file`.
#[derive(Debug)]
pub struct RemoveFileFuture<P>(Mode<P>)
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

impl<P> RemoveFileFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    fn new(path: P) -> RemoveFileFuture<P> {
        RemoveFileFuture(if tokio_threadpool::entered() {
            Mode::Native { path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::remove_file(&path))))
        })
    }
}

impl<P> Future for RemoveFileFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { path } => crate::blocking_io(|| fs::remove_file(path)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
