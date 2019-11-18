use super::*;
use rustls_dep::Session;

/// A wrapper around an underlying raw stream which implements the TLS or SSL
/// protocol.
#[derive(Debug)]
pub struct TlsStream<IO> {
    pub(crate) io: IO,
    pub(crate) session: ClientSession,
    pub(crate) state: TlsState,
}

pub(crate) enum MidHandshake<IO> {
    Handshaking(TlsStream<IO>),
    End,
}

impl<IO> TlsStream<IO> {
    /// Returns a shared reference to the inner stream and session.
    #[inline]
    pub fn get_ref(&self) -> (&IO, &ClientSession) {
        (&self.io, &self.session)
    }

    /// Returns a mutable reference to the inner stream and session.
    #[inline]
    pub fn get_mut(&mut self) -> (&mut IO, &mut ClientSession) {
        (&mut self.io, &mut self.session)
    }

    /// Returns a move to the inner stream and session.
    #[inline]
    pub fn into_inner(self) -> (IO, ClientSession) {
        (self.io, self.session)
    }
}

impl<IO> Future for MidHandshake<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    type Output = io::Result<TlsStream<IO>>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        if let MidHandshake::Handshaking(stream) = this {
            if !stream.state.is_early_data() {
                let eof = !stream.state.readable();
                let (io, session) = stream.get_mut();
                let mut stream = Stream::new(io, session).set_eof(eof);

                while stream.session.is_handshaking() {
                    futures::ready!(stream.handshake(cx))?;
                }

                while stream.session.wants_write() {
                    futures::ready!(stream.write_io(cx))?;
                }
            }
        }

        match mem::replace(this, MidHandshake::End) {
            MidHandshake::Handshaking(stream) => Poll::Ready(Ok(stream)),
            MidHandshake::End => panic!(),
        }
    }
}

impl<IO> AsyncRead for TlsStream<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [std::mem::MaybeUninit<u8>]) -> bool {
        self.io.prepare_uninitialized_buffer(buf)
    }

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match self.state {
            #[cfg(feature = "early-data")]
            TlsState::EarlyData(..) => Poll::Pending,
            TlsState::Stream | TlsState::WriteShutdown => {
                let this = self.get_mut();
                let mut stream =
                    Stream::new(&mut this.io, &mut this.session).set_eof(!this.state.readable());

                match stream.as_mut_pin().poll_read(cx, buf) {
                    Poll::Ready(Ok(0)) => {
                        this.state.shutdown_read();
                        Poll::Ready(Ok(0))
                    }
                    Poll::Ready(Ok(n)) => Poll::Ready(Ok(n)),
                    Poll::Ready(Err(ref e)) if e.kind() == io::ErrorKind::ConnectionAborted => {
                        this.state.shutdown_read();
                        if this.state.writeable() {
                            stream.session.send_close_notify();
                            this.state.shutdown_write();
                        }
                        Poll::Ready(Ok(0))
                    }
                    output => output,
                }
            }
            TlsState::ReadShutdown | TlsState::FullyShutdown => Poll::Ready(Ok(0)),
        }
    }
}

impl<IO> AsyncWrite for TlsStream<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    /// Note: that it does not guarantee the final data to be sent.
    /// To be cautious, you must manually call `flush`.
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let mut stream =
            Stream::new(&mut this.io, &mut this.session).set_eof(!this.state.readable());

        match this.state {
            #[cfg(feature = "early-data")]
            TlsState::EarlyData(ref mut pos, ref mut data) => {
                use std::io::Write;

                // write early data
                if let Some(mut early_data) = stream.session.early_data() {
                    let len = match early_data.write(buf) {
                        Ok(n) => n,
                        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                            return Poll::Pending
                        }
                        Err(err) => return Poll::Ready(Err(err)),
                    };
                    if len != 0 {
                        data.extend_from_slice(&buf[..len]);
                        return Poll::Ready(Ok(len));
                    }
                }

                // complete handshake
                while stream.session.is_handshaking() {
                    futures::ready!(stream.handshake(cx))?;
                }

                // write early data (fallback)
                if !stream.session.is_early_data_accepted() {
                    while *pos < data.len() {
                        let len =
                            futures::ready!(stream.as_mut_pin().poll_write(cx, &data[*pos..]))?;
                        *pos += len;
                    }
                }

                // end
                this.state = TlsState::Stream;
                stream.as_mut_pin().poll_write(cx, buf)
            }
            _ => stream.as_mut_pin().poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let mut stream =
            Stream::new(&mut this.io, &mut this.session).set_eof(!this.state.readable());

        #[cfg(feature = "early-data")]
        {
            if let TlsState::EarlyData(ref mut pos, ref mut data) = this.state {
                // complete handshake
                while stream.session.is_handshaking() {
                    futures::ready!(stream.handshake(cx))?;
                }

                // write early data (fallback)
                if !stream.session.is_early_data_accepted() {
                    while *pos < data.len() {
                        let len =
                            futures::ready!(stream.as_mut_pin().poll_write(cx, &data[*pos..]))?;
                        *pos += len;
                    }
                }

                this.state = TlsState::Stream;
            }
        }

        stream.as_mut_pin().poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.state.writeable() {
            self.session.send_close_notify();
            self.state.shutdown_write();
        }

        #[cfg(feature = "early-data")]
        {
            // we skip the handshake
            if let TlsState::EarlyData(..) = self.state {
                return Pin::new(&mut self.io).poll_shutdown(cx);
            }
        }

        let this = self.get_mut();
        let mut stream =
            Stream::new(&mut this.io, &mut this.session).set_eof(!this.state.readable());
        stream.as_mut_pin().poll_shutdown(cx)
    }
}
