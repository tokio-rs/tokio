use crate::codec::decoder::Decoder;
use crate::codec::encoder::Encoder;

use futures_core::Stream;
use tokio::io::{AsyncRead, AsyncWrite};

use bytes::BytesMut;
use futures_core::ready;
use futures_sink::Sink;
use log::trace;
use pin_project_lite::pin_project;
use std::borrow::{Borrow, BorrowMut};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    #[derive(Debug)]
    pub(crate) struct FramedImpl<T, U, State> {
        #[pin]
        pub(crate) inner: T,
        pub(crate) state: State,
        pub(crate) codec: U,
    }
}

const INITIAL_CAPACITY: usize = 8 * 1024;
const BACKPRESSURE_BOUNDARY: usize = INITIAL_CAPACITY;

pub(crate) struct ReadFrame {
    pub(crate) eof: bool,
    pub(crate) is_readable: bool,
    pub(crate) buffer: BytesMut,
}

pub(crate) struct WriteFrame {
    pub(crate) buffer: BytesMut,
}

#[derive(Default)]
pub(crate) struct RWFrames {
    pub(crate) read: ReadFrame,
    pub(crate) write: WriteFrame,
}

impl Default for ReadFrame {
    fn default() -> Self {
        Self {
            eof: false,
            is_readable: false,
            buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
        }
    }
}

impl Default for WriteFrame {
    fn default() -> Self {
        Self {
            buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
        }
    }
}

impl From<BytesMut> for ReadFrame {
    fn from(mut buffer: BytesMut) -> Self {
        let size = buffer.capacity();
        if size < INITIAL_CAPACITY {
            buffer.reserve(INITIAL_CAPACITY - size);
        }

        Self {
            buffer,
            is_readable: size > 0,
            eof: false,
        }
    }
}

impl From<BytesMut> for WriteFrame {
    fn from(mut buffer: BytesMut) -> Self {
        let size = buffer.capacity();
        if size < INITIAL_CAPACITY {
            buffer.reserve(INITIAL_CAPACITY - size);
        }

        Self { buffer }
    }
}

impl Borrow<ReadFrame> for RWFrames {
    fn borrow(&self) -> &ReadFrame {
        &self.read
    }
}
impl BorrowMut<ReadFrame> for RWFrames {
    fn borrow_mut(&mut self) -> &mut ReadFrame {
        &mut self.read
    }
}
impl Borrow<WriteFrame> for RWFrames {
    fn borrow(&self) -> &WriteFrame {
        &self.write
    }
}
impl BorrowMut<WriteFrame> for RWFrames {
    fn borrow_mut(&mut self) -> &mut WriteFrame {
        &mut self.write
    }
}
impl<T, U, R> Stream for FramedImpl<T, U, R>
where
    T: AsyncRead,
    U: Decoder,
    R: BorrowMut<ReadFrame>,
{
    type Item = Result<U::Item, U::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use crate::util::poll_read_buf;

        let mut pinned = self.project();
        let state: &mut ReadFrame = pinned.state.borrow_mut();
        // The following loops implements a state machine with each state corresponding
        // to a combination of the `is_readable` and `eof` flags. States persist across
        // loop entries and most state transitions occur with a return.
        //
        // The intitial state is `reading`.
        //
        // | state   | eof   | is_readable |
        // |---------|-------|-------------|
        // | reading | false | false       |
        // | framing | false | true        |
        // | pausing | true  | true        |
        // | paused  | true  | false       |
        //
        //                                 `decode_eof`
        //                                 returns `Some`             read 0 bytes
        //                                   │       │                 │      │
        //                                   │       ▼                 │      ▼
        //                                   ┌───────┐ `decode_eof`    ┌──────┐
        //                 ┌──read 0 bytes──▶│pausing│─returns `None`─▶│paused│──┐
        //                 │                 └───────┘                 └──────┘  │
        // pending read┐   │                     ┌──────┐                  │  ▲  │
        //      │      │   │                     │      │                  │  │  │
        //      │      ▼   │                     │  `decode` returns `Some`│ pending read
        //      │  ╔═══════╗                 ┌───────┐◀─┘                  │
        //      └──║reading║─read n>0 bytes─▶│framing│                     │
        //         ╚═══════╝                 └───────┘◀──────read n>0 bytes┘
        //             ▲                         │
        //             │                         │
        //             └─`decode` returns `None`─┘
        loop {
            // Repeatedly call `decode` or `decode_eof` while the buffer is "readable",
            // i.e. it _might_ contain data consumable as a frame or closing frame.
            // Both signal that there is no such data by returning `None`.
            //
            // If `decode` couldn't read a frame and the upstream source has returned eof,
            // `decode_eof` will attemp to decode the remaining bytes as closing frames.
            //
            // If the underlying AsyncRead is resumable, we may continue after an EOF,
            // but must finish emmiting all of it's associated `decode_eof` frames.
            // Furthermore, we don't want to emit any `decode_eof` frames on retried
            // reads after an EOF unless we've actually read more data.
            if state.is_readable {
                // pausing or framing
                if state.eof {
                    // pausing
                    let frame = pinned.codec.decode_eof(&mut state.buffer)?;
                    if frame.is_none() {
                        state.is_readable = false; // prepare pausing -> paused
                    }
                    // implicit pausing -> pausing or pausing -> paused
                    return Poll::Ready(frame.map(Ok));
                }

                // framing
                trace!("attempting to decode a frame");

                if let Some(frame) = pinned.codec.decode(&mut state.buffer)? {
                    trace!("frame decoded from buffer");
                    // implicit framing -> framing
                    return Poll::Ready(Some(Ok(frame)));
                }

                // framing -> reading
                state.is_readable = false;
            }
            // reading or paused
            // If we can't build a frame yet, try to read more data and try again.
            // Make sure we've got room for at least one byte to read to ensure
            // that we don't get a spurious 0 that looks like EOF.
            state.buffer.reserve(1);
            let bytect = match poll_read_buf(pinned.inner.as_mut(), cx, &mut state.buffer)? {
                Poll::Ready(ct) => ct,
                // implicit reading -> reading or implicit paused -> paused
                Poll::Pending => return Poll::Pending,
            };
            if bytect == 0 {
                if state.eof {
                    // We're already at an EOF, and since we've reached this path
                    // we're also not readable. This implies that we've already finished
                    // our `decode_eof` handling, so we can simply return `None`.
                    // implicit paused -> paused
                    return Poll::Ready(None);
                }
                // prepare reading -> paused
                state.eof = true;
            } else {
                // prepare paused -> framing or noop reading -> framing
                state.eof = false;
            }

            // paused -> framing or reading -> framing or reading -> pausing
            state.is_readable = true;
        }
    }
}

impl<T, I, U, W> Sink<I> for FramedImpl<T, U, W>
where
    T: AsyncWrite,
    U: Encoder<I>,
    U::Error: From<io::Error>,
    W: BorrowMut<WriteFrame>,
{
    type Error = U::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.state.borrow().buffer.len() >= BACKPRESSURE_BOUNDARY {
            self.as_mut().poll_flush(cx)
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        let pinned = self.project();
        pinned
            .codec
            .encode(item, &mut pinned.state.borrow_mut().buffer)?;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        use crate::util::poll_write_buf;
        trace!("flushing framed transport");
        let mut pinned = self.project();

        while !pinned.state.borrow_mut().buffer.is_empty() {
            let WriteFrame { buffer } = pinned.state.borrow_mut();
            trace!("writing; remaining={}", buffer.len());

            let n = ready!(poll_write_buf(pinned.inner.as_mut(), cx, buffer))?;

            if n == 0 {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to \
                     write frame to transport",
                )
                .into()));
            }
        }

        // Try flushing the underlying IO
        ready!(pinned.inner.poll_flush(cx))?;

        trace!("framed transport flushed");
        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.as_mut().poll_flush(cx))?;
        ready!(self.project().inner.poll_shutdown(cx))?;

        Poll::Ready(Ok(()))
    }
}
