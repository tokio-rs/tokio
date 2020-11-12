use crate::codec::{Decoder, ReadFrame};
use crate::codec::{Encoder, WriteFrame};

use pin_project_lite::pin_project;
use tokio::{io::ReadBuf, net::UdpSocket};
use tokio_stream::Stream;

use bytes::BufMut;
use futures_core::ready;
use futures_sink::Sink;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{borrow::BorrowMut, net::SocketAddr};
use std::{io, mem::MaybeUninit};

pin_project! {
#[derive(Debug)]
pub(crate) struct UdpFramedImpl<U, State> {
    #[pin]
    pub(crate) inner: UdpSocket,
    pub(crate) state: State,
    pub(crate) codec: U,
    pub(crate) current_addr: Option<SocketAddr>,
    pub(crate) out_addr: SocketAddr,
    pub(crate) flushed: bool,
}
}

pub(crate) const INITIAL_RD_CAPACITY: usize = 64 * 1024;
pub(crate) const INITIAL_WR_CAPACITY: usize = 8 * 1024;

impl<C, R> Stream for UdpFramedImpl<C, R>
where
    C: Decoder,
    R: BorrowMut<ReadFrame>,
{
    type Item = Result<(C::Item, SocketAddr), C::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut pin = self.project();

        let read_state: &mut ReadFrame = pin.state.borrow_mut();
        read_state.buffer.reserve(INITIAL_RD_CAPACITY);

        loop {
            // Are there are still bytes left in the read buffer to decode?
            if read_state.is_readable {
                if let Some(frame) = pin.codec.decode_eof(&mut read_state.buffer)? {
                    let current_addr = pin
                        .current_addr
                        .expect("will always be set before this line is called");

                    return Poll::Ready(Some(Ok((frame, current_addr))));
                }

                // if this line has been reached then decode has returned `None`.
                read_state.is_readable = false;
                read_state.buffer.clear();
            }

            // We're out of data. Try and fetch more data to decode
            let addr = unsafe {
                // Convert `&mut [MaybeUnit<u8>]` to `&mut [u8]` because we will be
                // writing to it via `poll_recv_from` and therefore initializing the memory.
                let buf =
                    &mut *(read_state.buffer.bytes_mut() as *mut _ as *mut [MaybeUninit<u8>]);
                let mut read = ReadBuf::uninit(buf);
                let ptr = read.filled().as_ptr();
                let res = ready!(Pin::new(&mut pin.inner).poll_recv_from(cx, &mut read));

                assert_eq!(ptr, read.filled().as_ptr());
                let addr = res?;
                read_state.buffer.advance_mut(read.filled().len());
                addr
            };

            *pin.current_addr = Some(addr);
            read_state.is_readable = true;
        }
    }
}

impl<I, C, W> Sink<(I, SocketAddr)> for UdpFramedImpl<C, W>
where
    C: Encoder<I>,
    C::Error: From<io::Error>,
    W: BorrowMut<WriteFrame>,
{
    type Error = C::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if !self.flushed {
            match self.poll_flush(cx)? {
                Poll::Ready(()) => {}
                Poll::Pending => return Poll::Pending,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: (I, SocketAddr)) -> Result<(), Self::Error> {
        let (frame, out_addr) = item;

        let pin = self.project();
        let write_state: &mut WriteFrame = pin.state.borrow_mut();

        pin.codec.encode(frame, &mut write_state.buffer)?;
        *pin.out_addr = out_addr;
        *pin.flushed = false;

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let pin = self.project();
        if *pin.flushed {
            return Poll::Ready(Ok(()));
        }

        let write_state: &mut WriteFrame = pin.state.borrow_mut();
        let n = ready!(pin
            .inner
            .poll_send_to(cx, &write_state.buffer, *pin.out_addr))?;

        let wrote_all = n == write_state.buffer.len();
        write_state.buffer.clear();
        *pin.flushed = true;

        let res = if wrote_all {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to write entire datagram to socket",
            )
            .into())
        };

        Poll::Ready(res)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_flush(cx))?;
        Poll::Ready(Ok(()))
    }
}
