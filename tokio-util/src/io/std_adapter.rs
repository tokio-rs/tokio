use pin_project_lite::pin_project;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;

/// Adapts an implementor of [`AsyncWrite`] to be usable in some context requiring [`std::io::Write`]
///
/// **Important**: this adapter is limited to a specific scenario where the calling code supports
/// restarting. In particular, the calling code MUST NOT call the `write_all` method on the writer.
#[derive(Debug)]
pub struct StdWriteAdapter<'a, 'b, W> {
    writer: W,
    cx: &'a mut Context<'b>,
}

impl<'a, 'b, W: AsyncWrite> StdWriteAdapter<'a, 'b, W> {
    /// Constructs the adapter.
    ///
    /// This method produces a restricted implementation of [`std::io::Write`] and may only be
    /// passed to functions that properly implement restarting and don't block. This property is
    /// neither checked by the type system nor required by the `Write` trait and thus it has to be
    /// explicitly guaranteed by each function that takes the returned value as argument.
    pub fn new_assuming_correct_usage(writer: W, cx: &'a mut Context<'b>) -> Self {
        Self {
            writer,
            cx,
        }
    }
}

impl<'a, 'b, W: AsyncWrite + Unpin> std::io::Write for StdWriteAdapter<'a, 'b, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match Pin::new(&mut self.writer).poll_write(self.cx, buf) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into()),
        }
    }

    /// Calling this method would be wrong therefore it always panics.
    #[track_caller]
    fn write_all(&mut self, _: &[u8]) -> std::io::Result<()> {
        panic!("operation not supported by the adapter")
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match Pin::new(&mut self.writer).poll_flush(self.cx) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(std::io::ErrorKind::WouldBlock.into()),
        }
    }
}

pin_project! {
    /// An asynchronous operation implemented using a restricted function taking `std::io::Write`.
    ///
    /// This is a [`Future`] that calls `F` each time it's polled, passing in a restricted writer.
    /// The closure `F` may pass this writer to other functions which require `std::io::Write` as
    /// long as those functions guarantee correct restarting in the presence of `WouldBlock`
    /// errors.
    ///
    /// E.g. given `fn perform_single_write(&mut self, writer: impl Write) -> io::Result<()>` which
    /// guarantees to keep consistent state in `self` even if `writer` returns error, one might
    /// write code like this:
    ///
    /// ```no-compile
    /// StdWriteAdapterFuture::new_assuming_correct_usage(writer, |writer| {
    ///     if self.needs_write() {
    ///         match self.perform_single_write() {
    ///             Ok(()) => (),
    ///             Err(error) if error.kind() == ErrorKind::WouldBlock => {
    ///                 Poll::Pending
    ///             }
    ///             Err(error) => Poll::Ready(Err(error)),
    ///         }
    ///     } else {
    ///         Poll::Ready(Ok(()))
    ///     }
    /// })
    /// ```
    pub struct StdWriteAdapterFuture<W, T, F> {
        #[pin]
        writer: W,
        f: F,
        _phantom: PhantomData<fn() -> T>,
    }
}

impl<W: AsyncWrite, T, F: FnMut(StdWriteAdapter<'_, '_, Pin<&mut W>>) -> Poll<T>> StdWriteAdapterFuture<W, T, F> {
    /// Constructs the future.
    ///
    /// It is required that the closure `f` only passes the writer given to it to functions that
    /// guarantee correct restarting in case of `WouldBlock` errors. In particular, the `write_all`
    /// method on the writer MUST NOT be called (it will panic) but also any other code that
    /// behaves similarly by keeping the state on the stack and losing it if error happens (easy to
    /// do by accident unless specific care was taken to avoid it).
    pub fn new_assuming_correct_usage(writer: W, f: F) -> Self {
        Self {
            writer,
            f,
            _phantom: PhantomData,
        }
    }
}

impl<W: AsyncWrite, T, F: FnMut(StdWriteAdapter<'_, '_, Pin<&mut W>>) -> Poll<T>> Future for StdWriteAdapterFuture<W, T, F> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let this = self.project();
        (this.f)(StdWriteAdapter::new_assuming_correct_usage(this.writer, cx))
    }
}
