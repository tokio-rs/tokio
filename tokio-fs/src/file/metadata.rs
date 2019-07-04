use super::File;
use std::future::Future;
use std::task::Poll;
use std::task::Context;
use std::fs::File as StdFile;
use std::fs::Metadata;
use std::io;
use std::pin::Pin;

const POLL_AFTER_RESOLVE: &str = "Cannot poll MetadataFuture after it resolves";

/// Future returned by `File::metadata` and resolves to a `(File, Metadata)` instance.
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
    type Output = io::Result<(File, Metadata)>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = Pin::get_mut(self);
        let metadata = ready!(crate::blocking_io(|| StdFile::metadata(inner.std())))?;

        let file = inner.file.take().expect(POLL_AFTER_RESOLVE);
        Poll::Ready(Ok((file, metadata).into()))
    }
}
