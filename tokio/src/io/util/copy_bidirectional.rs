cfg_io_util! {
    use super::copy::CopyBuffer;

    use crate::io::{AsyncRead, AsyncWrite};

    use std::task::{Context, Poll};
    use std::io;
    use std::pin::Pin;

    enum TransferState {
        Running(CopyBuffer),
        ShuttingDown(u64),
        Done(u64)
    }

    struct CopyBidirectional<'a, A: ?Sized, B: ?Sized> {
        a: &'a mut A,
        b: &'a mut B,
        a_to_b: TransferState,
        b_to_a: TransferState
    }

    fn transfer_one_direction<A, B>(
        cx: &mut Context<'_>,
        state: &mut TransferState,
        r: &mut A,
        w: &mut B
    ) -> Poll<io::Result<u64>>
        where A:  AsyncRead + AsyncWrite + Unpin + ?Sized, B: AsyncRead + AsyncWrite + Unpin + ?Sized
    {
        let mut r = Pin::new(r);
        let mut w = Pin::new(w);

        loop {
            match state {
                TransferState::Running(buf) => {
                    *state = TransferState::ShuttingDown(ready!(dbg!(buf.poll_copy(cx, r.as_mut(), w.as_mut())))?);
                }
                TransferState::ShuttingDown(count) => {
                    ready!(w.as_mut().poll_shutdown(cx))?;

                    *state = TransferState::Done(*count);
                }
                TransferState::Done(count) => break Poll::Ready(Ok(*count)),
            }
        }
    }

    fn swap_result<R, E>(r: Poll<Result<R,E>>) -> Result<Poll<R>, E> {
        match r {
            Poll::Pending => Ok(Poll::Pending),
            Poll::Ready(Ok(r)) => Ok(Poll::Ready(r)),
            Poll::Ready(Err(e)) => Err(e),
        }
    }

    impl<'a, A, B> std::future::Future for CopyBidirectional<'a, A, B>
        where A: AsyncRead + AsyncWrite + Unpin + ?Sized, B: AsyncRead + AsyncWrite + Unpin + ?Sized
    {
        type Output = io::Result<(u64, u64)>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            // Unpack self into mut refs to each field to avoid borrow check issues.
            let CopyBidirectional { a, b, a_to_b, b_to_a } = &mut *self;

            let a_to_b = swap_result(transfer_one_direction(cx, a_to_b, &mut *a, &mut *b))?;
            let b_to_a = swap_result(transfer_one_direction(cx, b_to_a, &mut *b, &mut *a))?;

            let a_to_b = ready!(a_to_b);
            let b_to_a = ready!(b_to_a);

            Ok((a_to_b, b_to_a)).into()
        }
    }

    /// Copies data in both directions between `a` and `b`.
    ///
    /// This function returns a future which will read from both streams, and
    /// write any data read to the opposing stream. The data transfer in one
    /// direction (eg, `a` to `b`) will not be blocked if the opposite direction
    /// blocks (e.g. if writing from `b` to `a` is blocked, `a` to `b` will not
    /// be blocked).
    ///
    /// If an EOF is observed on one stream, [`shutdown()`] will be invoked on
    /// the other, and reading from that stream will stop. Copying of data in
    /// the other direction will continue.
    ///
    /// The future will complete successfully once both streams return EOF,
    /// returning a tuple of the number of bytes copied from `a` to `b` and the
    /// number of bytes copied from `b` to `a`, in that order.
    ///
    /// [`shutdown()`]: crate::io::AsyncWriteExt::shutdown
    ///
    /// # Errors
    ///
    /// The future will immediately return an error if any IO operation on `a`
    /// or `b` returns an error. Some data read from either stream may be lost (not
    /// written to the other stream) in this case.
    ///
    /// # Return value
    ///
    /// Returns a tuple of bytes copied `a` to `b` and bytes copied `b` to `a`.
    pub async fn copy_bidirectional<A,B>(a: &mut A, b: &mut B)
        -> Result<(u64, u64), std::io::Error>
    where A: AsyncRead + AsyncWrite + Unpin + ?Sized, B: AsyncRead + AsyncWrite + Unpin + ?Sized
    {
        CopyBidirectional {
            a,
            b,
            a_to_b: TransferState::Running(CopyBuffer::new()),
            b_to_a: TransferState::Running(CopyBuffer::new()),
        }.await
    }
}
