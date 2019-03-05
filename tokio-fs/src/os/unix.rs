//! Unix-specific extensions to primitives in the `tokio_fs` module.

use crate::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::io;
use std::os::unix::fs;
use std::path::Path;

/// Creates a new symbolic link on the filesystem.
///
/// The `dst` path will be a symbolic link pointing to the `src` path.
///
/// This is an async version of [`std::os::unix::fs::symlink`][std]
///
/// [std]: https://doc.rust-lang.org/std/os/unix/fs/fn.symlink.html
pub fn symlink<P, Q>(src: P, dst: Q) -> SymlinkFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    SymlinkFuture::new(src, dst)
}

/// Future returned by `symlink`.
#[derive(Debug)]
pub struct SymlinkFuture<P, Q>(Mode<P, Q>)
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static;

#[derive(Debug)]
enum Mode<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    Native { src: P, dst: Q },
    Fallback(Blocking<(), io::Error>),
}

impl<P, Q> SymlinkFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    fn new(src: P, dst: Q) -> SymlinkFuture<P, Q> {
        SymlinkFuture(if tokio_threadpool::entered() {
            Mode::Native { src, dst }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::symlink(&src, &dst))))
        })
    }
}

impl<P, Q> Future for SymlinkFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { src, dst } => crate::blocking_io(|| fs::symlink(src, dst)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
