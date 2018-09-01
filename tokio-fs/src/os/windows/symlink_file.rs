use std::io;
use std::path::Path;
use std::os::windows::fs;

use futures::{Future, Poll};

/// Creates a new file symbolic link on the filesystem.
///
/// The `dst` path will be a file symbolic link pointing to the `src`
/// path.
///
/// This is an async version of [`std::os::windows::fs::symlink_file`][std]
///
/// [std]: https://doc.rust-lang.org/std/os/windows/fs/fn.symlink_file.html
pub fn symlink_file<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> SymlinkFileFuture<P, Q> {
    SymlinkFileFuture::new(src, dst)
}

/// Future returned by `symlink_file`.
#[derive(Debug)]
pub struct SymlinkFileFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    src: P,
    dst: Q,
}

impl<P, Q> SymlinkFileFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    fn new(src: P, dst: Q) -> SymlinkFileFuture<P, Q> {
        SymlinkFileFuture {
            src: src,
            dst: dst,
        }
    }
}

impl<P, Q> Future for SymlinkFileFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::symlink_file(&self.src, &self.dst) )
    }
}
