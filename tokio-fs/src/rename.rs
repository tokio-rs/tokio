use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs;
use std::io;
use std::path::Path;

/// Rename a file or directory to a new name, replacing the original file if
/// `to` already exists.
///
/// This will not work if the new name is on a different mount point.
///
/// This is an async version of [`std::fs::rename`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.rename.html
pub fn rename<P, Q>(from: P, to: Q) -> RenameFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    RenameFuture::new(from, to)
}

/// Future returned by `rename`.
#[derive(Debug)]
pub struct RenameFuture<P, Q>(Mode<P, Q>)
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static;

#[derive(Debug)]
enum Mode<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    Native { from: P, to: Q },
    Fallback(Blocking<(), io::Error>),
}

impl<P, Q> RenameFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    fn new(from: P, to: Q) -> RenameFuture<P, Q> {
        RenameFuture(if tokio_threadpool::entered() {
            Mode::Native { from, to }
        } else {
            Mode::Fallback(blocking(future::lazy(move || {
                fs::rename(&from, &to)
            })))
        })
    }
}

impl<P, Q> Future for RenameFuture<P, Q>
where
    P: AsRef<Path> + Send,
    Q: AsRef<Path> + Send,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { from, to } => crate::blocking_io(|| fs::rename(from, to)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
