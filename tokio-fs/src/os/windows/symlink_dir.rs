use blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::io;
use std::os::windows::fs;
use std::path::Path;

/// Creates a new directory symlink on the filesystem.
///
/// The `dst` path will be a directory symbolic link pointing to the `src`
/// path.
///
/// This is an async version of [`std::os::windows::fs::symlink_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/os/windows/fs/fn.symlink_dir.html
pub fn symlink_dir<P, Q>(src: P, dst: Q) -> SymlinkDirFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    SymlinkDirFuture::new(src, dst)
}

/// Future returned by `symlink_dir`.
#[derive(Debug)]
pub struct SymlinkDirFuture<P, Q>(Mode<P, Q>)
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

impl<P, Q> SymlinkDirFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    fn new(src: P, dst: Q) -> SymlinkDirFuture<P, Q> {
        SymlinkDirFuture(if tokio_threadpool::entered() {
            Mode::Native { src, dst }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::symlink_dir(&src, &dst))))
        })
    }
}

impl<P, Q> Future for SymlinkDirFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { src, dst } => crate::blocking_io(|| fs::symlink_dir(src, dst)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
