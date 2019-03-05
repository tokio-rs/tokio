use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Reads a symbolic link, returning the file that the link points to.
///
/// This is an async version of [`std::fs::read_link`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.read_link.html
pub fn read_link<P>(path: P) -> ReadLinkFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    ReadLinkFuture::new(path)
}

/// Future returned by `read_link`.
#[derive(Debug)]
pub struct ReadLinkFuture<P>(Mode<P>)
where
    P: AsRef<Path> + Send + 'static;

#[derive(Debug)]
enum Mode<P>
where
    P: AsRef<Path> + Send + 'static,
{
    Native { path: P },
    Fallback(Blocking<PathBuf, io::Error>),
}

impl<P> ReadLinkFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    fn new(path: P) -> ReadLinkFuture<P> {
        ReadLinkFuture(if tokio_threadpool::entered() {
            Mode::Native { path }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::read_link(path))))
        })
    }
}

impl<P> Future for ReadLinkFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = PathBuf;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { path } => crate::blocking_io(|| fs::read_link(path)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
