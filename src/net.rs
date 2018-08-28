//! TCP/UDP/Unix bindings for `tokio`.
//!
//! This module contains the TCP/UDP/Unix networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! # Organization
//!
//! * [`TcpListener`] and [`TcpStream`] provide functionality for communication over TCP
//! * [`UdpSocket`] and [`UdpFramed`] provide functionality for communication over UDP
//! * [`UnixListener`] and [`UnixStream`] provide functionality for communication over a
//! Unix Domain Socket **(available on Unix only)**
//!
//! [`TcpListener`]: struct.TcpListener.html
//! [`TcpStream`]: struct.TcpStream.html
//! [`UdpSocket`]: struct.UdpSocket.html
//! [`UdpFramed`]: struct.UdpFramed.html
//! [`UnixListener`]: struct.UnixListener.html
//! [`UnixStream`]: struct.UnixStream.html

pub mod tcp {
    //! TCP bindings for `tokio`.
    //!
    //! Connecting to an address, via TCP, can be done using [`TcpStream`]'s
    //! [`connect`] method, which returns [`ConnectFuture`]. `ConnectFuture`
    //! implements a future which returns a `TcpStream`.
    //!
    //! To listen on an address [`TcpListener`] can be used. `TcpListener`'s
    //! [`incoming`][incoming_method] method can be used to accept new connections.
    //! It return the [`Incoming`] struct, which implements a stream which returns
    //! `TcpStream`s.
    //!
    //! [`TcpStream`]: struct.TcpStream.html
    //! [`connect`]: struct.TcpStream.html#method.connect
    //! [`ConnectFuture`]: struct.ConnectFuture.html
    //! [`TcpListener`]: struct.TcpListener.html
    //! [incoming_method]: struct.TcpListener.html#method.incoming
    //! [`Incoming`]: struct.Incoming.html
    pub use tokio_tcp::{ConnectFuture, Incoming, TcpListener, TcpStream};
}
pub use self::tcp::{TcpListener, TcpStream};

#[deprecated(note = "use `tokio::net::tcp::ConnectFuture` instead")]
#[doc(hidden)]
pub type ConnectFuture = self::tcp::ConnectFuture;
#[deprecated(note = "use `tokio::net::tcp::Incoming` instead")]
#[doc(hidden)]
pub type Incoming = self::tcp::Incoming;

pub mod udp {
    //! UDP bindings for `tokio`.
    //!
    //! The main struct for UDP is the [`UdpSocket`], which represents a UDP socket.
    //! Reading and writing to it can be done using futures, which return the
    //! [`RecvDgram`] and [`SendDgram`] structs respectively.
    //!
    //! For convenience it's also possible to convert raw datagrams into higher-level
    //! frames.
    //!
    //! [`UdpSocket`]: struct.UdpSocket.html
    //! [`RecvDgram`]: struct.RecvDgram.html
    //! [`SendDgram`]: struct.SendDgram.html
    //! [`UdpFramed`]: struct.UdpFramed.html
    //! [`framed`]: struct.UdpSocket.html#method.framed
    pub use tokio_udp::{RecvDgram, SendDgram, UdpFramed, UdpSocket};
}
pub use self::udp::{UdpFramed, UdpSocket};

#[deprecated(note = "use `tokio::net::udp::RecvDgram` instead")]
#[doc(hidden)]
pub type RecvDgram<T> = self::udp::RecvDgram<T>;
#[deprecated(note = "use `tokio::net::udp::SendDgram` instead")]
#[doc(hidden)]
pub type SendDgram<T> = self::udp::SendDgram<T>;

#[cfg(unix)]
pub mod unix {
    //! Unix domain socket bindings for `tokio` (only available on unix systems).

    pub use tokio_uds::{
        ConnectFuture, Incoming, RecvDgram, SendDgram, UCred, UnixDatagram, UnixListener,
        UnixStream,
    };
}
#[cfg(unix)]
pub use self::unix::{UnixListener, UnixStream};
