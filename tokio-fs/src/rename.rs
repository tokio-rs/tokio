use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Rename a file or directory to a new name, replacing the original file if
/// `to` already exists.
///
/// This will not work if the new name is on a different mount point.
///
/// This is an async version of [`std::fs::rename`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.rename.html
pub fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> RenameFuture<P, Q> {
    RenameFuture::new(from, to)
}

/// Future returned by `rename`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct RenameFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    from: P,
    to: Q,
}

impl<P, Q> RenameFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    fn new(from: P, to: Q) -> RenameFuture<P, Q> {
        RenameFuture { from, to }
    }
}

impl<P, Q> Future for RenameFuture<P, Q>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        crate::blocking_io(|| fs::rename(&self.from, &self.to))
    }
}
