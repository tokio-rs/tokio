use super::blocking_pool::{blocking, Blocking};
use futures::{future, Future, Poll};
use std::fs;
use std::io;
use std::path::Path;

/// Changes the permissions found on a file or a directory.
///
/// This is an async version of [`std::fs::set_permissions`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.set_permissions.html
pub fn set_permissions<P>(path: P, perm: fs::Permissions) -> SetPermissionsFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    SetPermissionsFuture::new(path, perm)
}

/// Future returned by `set_permissions`.
#[derive(Debug)]
pub struct SetPermissionsFuture<P>(Mode<P>)
where
    P: AsRef<Path> + Send + 'static;

#[derive(Debug)]
enum Mode<P>
where
    P: AsRef<Path> + Send + 'static,
{
    Native { path: P, perm: fs::Permissions },
    Fallback(Blocking<(), io::Error>),
}

impl<P> SetPermissionsFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    fn new(path: P, perm: fs::Permissions) -> SetPermissionsFuture<P> {
        SetPermissionsFuture(if tokio_threadpool::entered() {
            Mode::Native { path, perm }
        } else {
            Mode::Fallback(blocking(future::lazy(move || {
                fs::set_permissions(&path, perm)
            })))
        })
    }
}

impl<P> Future for SetPermissionsFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match &mut self.0 {
            Mode::Native { path, perm } => {
                crate::blocking_io(|| fs::set_permissions(path, perm.clone()))
            }
            Mode::Fallback(job) => job.poll(),
        }
    }
}
