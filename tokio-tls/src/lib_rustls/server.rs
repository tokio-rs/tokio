use super::*;
use rustls_dep::Session;

/// A wrapper around an underlying raw stream which implements the TLS or SSL
/// protocol.
#[derive(Debug)]
pub struct TlsStream<IO> {
    pub(crate) io: IO,
    pub(crate) session: ServerSession,
    pub(crate) state: TlsState,
}

pub(crate) enum MidHandshake<IO> {
    Handshaking(TlsStream<IO>),
    End,
}

impl<IO> TlsStream<IO> {
    /// Returns a shared reference to the inner stream and session.
    #[inline]
    pub fn get_ref(&self) -> (&IO, &ServerSession) {
        (&self.io, &self.session)
    }

    /// Returns a mutable reference to the inner stream and session.
    #[inline]
    pub fn get_mut(&mut self) -> (&mut IO, &mut ServerSession) {
        (&mut self.io, &mut self.session)
    }

    /// Returns a move to the inner stream and session.
    #[inline]
    pub fn into_inner(self) -> (IO, ServerSession) {
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
        let this = self.get_mut();
        let mut stream =
            Stream::new(&mut this.io, &mut this.session).set_eof(!this.state.readable());

        match &this.state {
            TlsState::Stream | TlsState::WriteShutdown => {
                match stream.as_mut_pin().poll_read(cx, buf) {
                    Poll::Ready(Ok(0)) => {
                        this.state.shutdown_read();
                        Poll::Ready(Ok(0))
                    }
                    Poll::Ready(Ok(n)) => Poll::Ready(Ok(n)),
                    Poll::Ready(Err(ref err)) if err.kind() == io::ErrorKind::ConnectionAborted => {
                        this.state.shutdown_read();
                        if this.state.writeable() {
                            stream.session.send_close_notify();
                            this.state.shutdown_write();
                        }
                        Poll::Ready(Ok(0))
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                    Poll::Pending => Poll::Pending,
                }
            }
            TlsState::ReadShutdown | TlsState::FullyShutdown => Poll::Ready(Ok(0)),
            #[cfg(feature = "early-data")]
            s => unreachable!("server TLS can not hit this state: {:?}", s),
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
        stream.as_mut_pin().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let mut stream =
            Stream::new(&mut this.io, &mut this.session).set_eof(!this.state.readable());
        stream.as_mut_pin().poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.state.writeable() {
            self.session.send_close_notify();
            self.state.shutdown_write();
        }

        let this = self.get_mut();
        let mut stream =
            Stream::new(&mut this.io, &mut this.session).set_eof(!this.state.readable());
        stream.as_mut_pin().poll_shutdown(cx)
    }
}
