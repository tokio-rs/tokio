//! Async TLS streams
//!
//! This library is an implementation of TLS streams using the most appropriate
//! system library by default for negotiating the connection. That is, on
//! Windows this library uses SChannel, on OSX it uses SecureTransport, and on
//! other platforms it uses OpenSSL.
//!
//! Each TLS stream implements the `Read` and `Write` traits to interact and
//! interoperate with the rest of the futures I/O ecosystem. Client connections
//! initiated from this crate verify hostnames automatically and by default.
//!
//! This crate primarily exports this ability through two newtypes,
//! `TlsConnector` and `TlsAcceptor`. These newtypes augment the
//! functionality provided by the `native-tls` crate, on which this crate is
//! built. Configuration of TLS parameters is still primarily done through the
//! `native-tls` crate.

#![deny(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tokio-tls/0.2.0")]

extern crate futures;
extern crate native_tls;
#[macro_use]
extern crate tokio_io;

use std::io::{self, Read, Write};

use futures::{Poll, Future, Async};
use native_tls::{HandshakeError, Error};
use tokio_io::{AsyncRead, AsyncWrite};

/// A wrapper around an underlying raw stream which implements the TLS or SSL
/// protocol.
///
/// A `TlsStream<S>` represents a handshake that has been completed successfully
/// and both the server and the client are ready for receiving and sending
/// data. Bytes read from a `TlsStream` are decrypted from `S` and bytes written
/// to a `TlsStream` are encrypted when passing through to `S`.
#[derive(Debug)]
pub struct TlsStream<S> {
    inner: native_tls::TlsStream<S>,
}

/// A wrapper around a `native_tls::TlsConnector`, providing an async `connect`
/// method.
pub struct TlsConnector {
    inner: native_tls::TlsConnector,
}

/// A wrapper around a `native_tls::TlsAcceptor`, providing an async `accept`
/// method.
pub struct TlsAcceptor {
    inner: native_tls::TlsAcceptor,
}

/// Future returned from `TlsConnector::connect` which will resolve
/// once the connection handshake has finished.
pub struct Connect<S> {
    inner: MidHandshake<S>,
}

/// Future returned from `TlsAcceptor::accept` which will resolve
/// once the accept handshake has finished.
pub struct Accept<S> {
    inner: MidHandshake<S>,
}

struct MidHandshake<S> {
    inner: Option<Result<native_tls::TlsStream<S>, HandshakeError<S>>>,
}

impl<S> TlsStream<S> {
    /// Get access to the internal `native_tls::TlsStream` stream which also
    /// transitively allows access to `S`.
    pub fn get_ref(&self) -> &native_tls::TlsStream<S> {
        &self.inner
    }

    /// Get mutable access to the internal `native_tls::TlsStream` stream which
    /// also transitively allows mutable access to `S`.
    pub fn get_mut(&mut self) -> &mut native_tls::TlsStream<S> {
        &mut self.inner
    }
}

impl<S: Read + Write> Read for TlsStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<S: Read + Write> Write for TlsStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}


impl<S: AsyncRead + AsyncWrite> AsyncRead for TlsStream<S> {
}

impl<S: AsyncRead + AsyncWrite> AsyncWrite for TlsStream<S> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        try_nb!(self.inner.shutdown());
        self.inner.get_mut().shutdown()
    }
}

impl TlsConnector {
    /// Connects the provided stream with this connector, assuming the provided
    /// domain.
    ///
    /// This function will internally call `TlsConnector::connect` to connect
    /// the stream and returns a future representing the resolution of the
    /// connection operation. The returned future will resolve to either
    /// `TlsStream<S>` or `Error` depending if it's successful or not.
    ///
    /// This is typically used for clients who have already established, for
    /// example, a TCP connection to a remote server. That stream is then
    /// provided here to perform the client half of a connection to a
    /// TLS-powered server.
    pub fn connect<S>(&self, domain: &str, stream: S) -> Connect<S>
        where S: AsyncRead + AsyncWrite,
    {
        Connect {
            inner: MidHandshake {
                inner: Some(self.inner.connect(domain, stream)),
            },
        }
    }
}

impl From<native_tls::TlsConnector> for TlsConnector {
    fn from(inner: native_tls::TlsConnector) -> TlsConnector {
        TlsConnector {
            inner,
        }
    }
}

impl TlsAcceptor {
    /// Accepts a new client connection with the provided stream.
    ///
    /// This function will internally call `TlsAcceptor::accept` to connect
    /// the stream and returns a future representing the resolution of the
    /// connection operation. The returned future will resolve to either
    /// `TlsStream<S>` or `Error` depending if it's successful or not.
    ///
    /// This is typically used after a new socket has been accepted from a
    /// `TcpListener`. That socket is then passed to this function to perform
    /// the server half of accepting a client connection.
    pub fn accept<S>(&self, stream: S) -> Accept<S>
        where S: AsyncRead + AsyncWrite,
    {
        Accept {
            inner: MidHandshake {
                inner: Some(self.inner.accept(stream)),
            },
        }
    }
}

impl From<native_tls::TlsAcceptor> for TlsAcceptor {
    fn from(inner: native_tls::TlsAcceptor) -> TlsAcceptor {
        TlsAcceptor {
            inner,
        }
    }
}

impl<S: AsyncRead + AsyncWrite> Future for Connect<S> {
    type Item = TlsStream<S>;
    type Error = Error;

    fn poll(&mut self) -> Poll<TlsStream<S>, Error> {
        self.inner.poll()
    }
}

impl<S: AsyncRead + AsyncWrite> Future for Accept<S> {
    type Item = TlsStream<S>;
    type Error = Error;

    fn poll(&mut self) -> Poll<TlsStream<S>, Error> {
        self.inner.poll()
    }
}

impl<S: AsyncRead + AsyncWrite> Future for MidHandshake<S> {
    type Item = TlsStream<S>;
    type Error = Error;

    fn poll(&mut self) -> Poll<TlsStream<S>, Error> {
        match self.inner.take().expect("cannot poll MidHandshake twice") {
            Ok(stream) => Ok(TlsStream { inner: stream }.into()),
            Err(HandshakeError::Failure(e)) => Err(e),
            Err(HandshakeError::WouldBlock(s)) => {
                match s.handshake() {
                    Ok(stream) => Ok(TlsStream { inner: stream }.into()),
                    Err(HandshakeError::Failure(e)) => Err(e),
                    Err(HandshakeError::WouldBlock(s)) => {
                        self.inner = Some(Err(HandshakeError::WouldBlock(s)));
                        Ok(Async::NotReady)
                    }
                }
            }
        }
    }
}
