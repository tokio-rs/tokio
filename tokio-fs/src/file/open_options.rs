use super::File;

use futures::{Future, Poll};

use std::fs::OpenOptions;
use std::io;
use std::path::Path;

/// Future returned by `OpenOptionsExt::open_async` and resolves to a `File` instance.
#[derive(Debug)]
pub struct OpenOptionsFuture<P> {
    options: OpenOptions,
    path: P,
}

impl<P> OpenOptionsFuture<P>
where P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(options: OpenOptions, path: P) -> Self {
        OpenOptionsFuture { options, path }
    }
}

impl<P> Future for OpenOptionsFuture<P>
where P: AsRef<Path> + Send + 'static,
{
    type Item = File;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let std = try_ready!(::blocking_io(|| {
            self.options.open(&self.path)
        }));

        let file = File::from_std(std);
        Ok(file.into())
    }
}

pub trait OpenOptionsExt {
    /// Opens a file at `path` with the options specified by `self`.
    ///
    /// # Errors
    ///
    /// `OpenOptionsFuture` results in an error if called from outside of the
    /// Tokio runtime or if the underlying [`open`] call results in an error.
    ///
    /// [`open`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.open
    fn open_async<P>(&self, path: P) -> OpenOptionsFuture<P>
    where P: AsRef<Path> + Send + 'static;

    /// Opens a file at `path` with the options specified by `self`, consuming
    /// `self`.
    ///
    /// # Errors
    ///
    /// `OpenOptionsFuture` results in an error if called from outside of the
    /// Tokio runtime or if the underlying [`open`] call results in an error.
    ///
    /// [`open`]: https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.open
    fn into_open_async<P>(self, path: P) -> OpenOptionsFuture<P>
    where P: AsRef<Path> + Send + 'static;
}

impl OpenOptionsExt for OpenOptions {
    fn open_async<P: AsRef<Path>>(&self, path: P) -> OpenOptionsFuture<P>
    where P: AsRef<Path> + Send + 'static,
    {
        self.clone().into_open_async(path)
    }

    fn into_open_async<P: AsRef<Path>>(self, path: P) -> OpenOptionsFuture<P>
    where P: AsRef<Path> + Send + 'static,
    {
        OpenOptionsFuture::new(self, path)
    }
}
