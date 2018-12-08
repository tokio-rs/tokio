use std::fs;
use std::io;
use std::path::Path;

use futures::{Future, Poll};

/// Creates a new hard link on the filesystem.
///
/// The `dst` path will be a link pointing to the `src` path. Note that systems
/// often require these two paths to both be located on the same filesystem.
///
/// This is an async version of [`std::fs::hard_link`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.hard_link.html
pub fn hard_link<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) -> HardLinkFuture<P, Q> {
    HardLinkFuture::new(src, dst)
}

/// Future returned by `hard_link`.
#[derive(Debug)]
pub struct HardLinkFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    src: P,
    dst: Q,
}

impl<P, Q> HardLinkFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    fn new(src: P, dst: Q) -> HardLinkFuture<P, Q> {
        HardLinkFuture {
            src: src,
            dst: dst,
        }
    }
}

impl<P, Q> Future for HardLinkFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::hard_link(&self.src, &self.dst) )
    }
}
