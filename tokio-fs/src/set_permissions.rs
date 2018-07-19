use std::fs;
use std::io;
use std::path::Path;

use futures::{Future, Poll};

/// Changes the permissions found on a file or a directory.
///
/// This is an async version of [`std::fs::set_permissions`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.set_permissions.html
pub fn set_permissions<P: AsRef<Path>>(path: P, perm: fs::Permissions) -> SetPermissionsFuture<P> {
    SetPermissionsFuture::new(path, perm)
}

/// Future returned by `set_permissions`.
#[derive(Debug)]
pub struct SetPermissionsFuture<P>
where
    P: AsRef<Path>
{
    path: P,
    perm: fs::Permissions,
}

impl<P> SetPermissionsFuture<P>
where
    P: AsRef<Path>
{
    fn new(path: P, perm: fs::Permissions) -> SetPermissionsFuture<P> {
        SetPermissionsFuture {
            path: path,
            perm: perm,
        }
    }
}

impl<P> Future for SetPermissionsFuture<P>
where
    P: AsRef<Path>
{
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ::blocking_io(|| fs::set_permissions(&self.path, self.perm.clone()) )
    }
}
