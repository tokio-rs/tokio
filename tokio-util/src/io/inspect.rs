use futures_core::ready;
use pin_project_lite::pin_project;
use std::{
    io::Result,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, ReadBuf};

pin_project! {
    /// An adapter that lets you inspect the data that's being read.
    ///
    /// This is useful for things like hashing data as it's read in.
    pub struct InspectReader<R: AsyncRead, F: FnMut(&[u8])> {
        #[pin]
        reader: R,
        f: F,
    }
}

impl<R: AsyncRead, F: FnMut(&[u8])> InspectReader<R, F> {
    /// Create a new InspectReader, wrapping `reader` and calling `f` for the
    /// new data supplied by each read call.
    pub fn new(reader: R, f: F) -> InspectReader<R, F> {
        InspectReader { reader, f }
    }

    /// Consumes the `InspectReader`, returning the wrapped reader
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: AsyncRead, F: FnMut(&[u8])> AsyncRead for InspectReader<R, F> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        let me = self.project();
        let filled_length = buf.filled().len();
        ready!(me.reader.poll_read(cx, buf))?;
        (me.f)(&buf.filled()[filled_length..]);
        Poll::Ready(Ok(()))
    }
}
