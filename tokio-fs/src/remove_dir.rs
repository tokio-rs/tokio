use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Removes an existing, empty directory.
///
/// This is an async version of [`std::fs::remove_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.remove_dir.html
pub fn remove_dir<P: AsRef<Path>>(path: P) -> RemoveDirFuture<P> {
    RemoveDirFuture::new(path)
}

/// Future returned by `remove_dir`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct RemoveDirFuture<P>
where
    P: AsRef<Path>,
{
    path: P,
}

impl<P> RemoveDirFuture<P>
where
    P: AsRef<Path>,
{
    fn new(path: P) -> RemoveDirFuture<P> {
        RemoveDirFuture { path: path }
    }
}

impl<P> Future for RemoveDirFuture<P>
where
    P: AsRef<Path>,
{
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        crate::blocking_io(|| fs::remove_dir(&self.path))
    }
}
