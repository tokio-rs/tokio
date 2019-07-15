use tokio_io::AsyncBufRead;

/// An extension trait which adds utility methods to `AsyncBufRead` types.
pub trait AsyncBufReadExt: AsyncBufRead {}

impl<R: AsyncBufRead + ?Sized> AsyncBufReadExt for R {}
