use blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::io;
use std::os::windows::fs;
use std::path::Path;

/// Creates a new file symbolic link on the filesystem.
///
/// The `dst` path will be a file symbolic link pointing to the `src`
/// path.
///
/// This is an async version of [`std::os::windows::fs::symlink_file`][std]
///
/// [std]: https://doc.rust-lang.org/std/os/windows/fs/fn.symlink_file.html
pub fn symlink_file<P, Q>(src: P, dst: Q) -> SymlinkFileFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    SymlinkFileFuture::new(src, dst)
}

/// Future returned by `symlink_file`.
#[derive(Debug)]
pub struct SymlinkFileFuture<P, Q>(Mode<P, Q>)
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

impl<P, Q> SymlinkFileFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    fn new(src: P, dst: Q) -> SymlinkFileFuture<P, Q> {
        SymlinkFileFuture(if tokio_threadpool::entered() {
            Mode::Native { src, dst }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::symlink_file(&src, &dst))))
        })
    }
}

impl<P, Q> Future for SymlinkFileFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { src, dst } => crate::blocking_io(|| fs::symlink_file(src, dst)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
