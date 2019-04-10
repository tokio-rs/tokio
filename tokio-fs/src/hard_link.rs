use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs;
use std::io;
use std::path::Path;

/// Creates a new hard link on the filesystem.
///
/// The `dst` path will be a link pointing to the `src` path. Note that systems
/// often require these two paths to both be located on the same filesystem.
///
/// This is an async version of [`std::fs::hard_link`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.hard_link.html
pub fn hard_link<P, Q>(src: P, dst: Q) -> HardLinkFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    HardLinkFuture::new(src, dst)
}

/// Future returned by `hard_link`.
#[derive(Debug)]
pub struct HardLinkFuture<P, Q>(Mode<P, Q>)
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

impl<P, Q> HardLinkFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    fn new(src: P, dst: Q) -> HardLinkFuture<P, Q> {
        HardLinkFuture(if tokio_threadpool::entered() {
            Mode::Native { src, dst }
        } else {
            Mode::Fallback(blocking(future::lazy(move || fs::hard_link(&src, &dst))))
        })
    }
}

impl<P, Q> Future for HardLinkFuture<P, Q>
where
    P: AsRef<Path> + Send + 'static,
    Q: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { src, dst } => crate::blocking_io(|| fs::hard_link(src, dst)),
            Mode::Fallback(job) => job.poll(),
        }
    }
}
