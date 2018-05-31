use super::File;

use futures::{Future, Poll};

use std::fs::File as StdFile;
use std::fs::Metadata;
use std::io;

const POLL_AFTER_RESOLVE: &str = "Cannot poll MetadataFuture after it resolves";

/// Future returned by `File::metadata` and resolves to a `(Metadata, File)` instance.
#[derive(Debug)]
pub struct MetadataFuture {
    file: Option<File>,
}

impl MetadataFuture {
    pub(crate) fn new(file: File) -> Self {
        MetadataFuture { file: Some(file) }
    }

    fn std(&mut self) -> &mut StdFile {
        self.file.as_mut().expect(POLL_AFTER_RESOLVE).std()
    }
}

impl Future for MetadataFuture {
    type Item = (File, Metadata);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let metadata = try_ready!(::blocking_io(|| {
            StdFile::metadata(self.std())
        }));

        let file = self.file.take().expect(POLL_AFTER_RESOLVE);
        Ok((file, metadata).into())
    }
}
