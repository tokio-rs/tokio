use std::future::Future;
use std::io;
use std::os::windows::fs;
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Creates a new directory symlink on the filesystem.
///
/// The `dst` path will be a directory symbolic link pointing to the `src`
/// path.
///
/// This is an async version of [`std::os::windows::fs::symlink_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/os/windows/fs/fn.symlink_dir.html
pub fn symlink_dir<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> SymlinkDirFuture<P, Q> {
    SymlinkDirFuture::new(src, dst)
}

/// Future returned by `symlink_dir`.
#[derive(Debug)]
pub struct SymlinkDirFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    src: P,
    dst: Q,
}

impl<P, Q> SymlinkDirFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    fn new(src: P, dst: Q) -> SymlinkDirFuture<P, Q> {
        SymlinkDirFuture { src, dst }
    }
}

impl<P, Q> Future for SymlinkDirFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        crate::blocking_io(|| fs::symlink_dir(&self.src, &self.dst))
    }
}
