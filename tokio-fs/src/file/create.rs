use super::File;

use futures::{Future, Poll};

use std::fs::File as StdFile;
use std::io;
use std::path::Path;

/// Future returned by `File::create` and resolves to a `File` instance.
#[derive(Debug)]
pub struct CreateFuture<P> {
    path: P,
}

impl<P> CreateFuture<P>
where P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(path: P) -> Self {
        CreateFuture { path }
    }
}

impl<P> Future for CreateFuture<P>
where P: AsRef<Path> + Send + 'static,
{
    type Item = File;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let std = try_ready!(::blocking_io(|| {
            StdFile::create(&self.path)
        }));

        let file = File::from_std(std);
        Ok(file.into())
    }
}
