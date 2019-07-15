use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Creates a new, empty directory at the provided path
///
/// This is an async version of [`std::fs::create_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir.html
pub fn create_dir<P: AsRef<Path>>(path: P) -> CreateDirFuture<P> {
    CreateDirFuture::new(path)
}

/// Future returned by `create_dir`.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct CreateDirFuture<P>
where
    P: AsRef<Path>,
{
    path: P,
}

impl<P> CreateDirFuture<P>
where
    P: AsRef<Path>,
{
    fn new(path: P) -> CreateDirFuture<P> {
        CreateDirFuture { path: path }
    }
}

impl<P> Future for CreateDirFuture<P>
where
    P: AsRef<Path>,
{
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        crate::blocking_io(|| fs::create_dir(&self.path))
    }
}
