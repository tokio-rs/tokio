//! TCP/UDP bindings for `tokio`.
//!
//! This module contains the TCP/UDP networking types, similar to the standard
//! library, which can be used to implement networking protocols.
//!
//! # TCP
//!
//! Connecting to an address, via TCP, can be done using [`TcpStream`]'s
//! [`connect`] method, which returns [`TcpStreamNew`]. `TcpStreamNew`
//! implements a future which returns a `TcpStream`.
//!
//! To listen on an address [`TcpListener`] can be used. `TcpListener`'s
//! [`incoming`][incoming_method] method can be used to accept new connections.
//! It return the [`Incoming`] struct, which implements a stream which returns
//! `TcpStream`s.
//!
//! [`TcpStream`]: struct.TcpStream.html
//! [`connect`]: struct.TcpStream.html#method.connect
//! [`TcpStreamNew`]: struct.TcpStreamNew.html
//! [`TcpListener`]: struct.TcpListener.html
//! [incoming_method]: struct.TcpListener.html#method.incoming
//! [`Incoming`]: struct.Incoming.html
//!
//! # UDP
//!
//! The main struct for UDP is the [`UdpSocket`], which represents a UDP socket.
//! Reading and writing to it can be done using futures, which return the
//! [`RecvDgram`] and [`SendDgram`] structs respectively.
//!
//! For convience it's also possible to convert raw datagrams into higher-level
//! frames. This done with [`UdpFramed`], created by calling [`framed`] on a
//! [`UdpSocket`], and using the [`UdpCodec`].
//!
//! [`UdpSocket`]: struct.UdpSocket.html
//! [`RecvDgram`]: struct.RecvDgram.html
//! [`SendDgram`]: struct.SendDgram.html
//! [`UdpFramed`]: struct.UdpFramed.html
//! [`framed`]: struct.UdpSocket.html#method.framed
//! [`UdpCodec`]: trait.UdpCodec.html

mod tcp;
mod udp;

pub use self::tcp::{TcpStream, TcpStreamNew};
pub use self::tcp::{TcpListener, Incoming};
pub use self::udp::{UdpSocket, UdpCodec, UdpFramed, SendDgram, RecvDgram};
