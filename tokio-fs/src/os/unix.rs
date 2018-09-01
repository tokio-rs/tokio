//! Unix-specific extensions to primitives in the `tokio_fs` module.

use std::io;
use std::path::Path;
use std::os::unix::fs;

use futures::{Future, Poll};

/// Creates a new symbolic link on the filesystem.
///
/// The `dst` path will be a symbolic link pointing to the `src` path.
///
/// This is an async version of [`std::os::unix::fs::symlink`][std]
///
/// [std]: https://doc.rust-lang.org/std/os/unix/fs/fn.symlink.html
pub fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> SymlinkFuture<P, Q> {
    SymlinkFuture::new(src, dst)
}

/// Future returned by `symlink`.
#[derive(Debug)]
pub struct SymlinkFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    src: P,
    dst: Q,
}

impl<P, Q> SymlinkFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    fn new(src: P, dst: Q) -> SymlinkFuture<P, Q> {
        SymlinkFuture {
            src: src,
            dst: dst,
        }
    }
}

impl<P, Q> Future for SymlinkFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::symlink(&self.src, &self.dst) )
    }
}
