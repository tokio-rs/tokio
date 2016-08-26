//! Mio bindings with streams and futures
//!
//! This crate uses the `futures_io` and `futures` crates to provide a thin
//! binding on top of mio of TCP and UDP sockets.

#![deny(missing_docs)]

extern crate futures;
extern crate mio;
extern crate slab;

#[macro_use]
extern crate scoped_tls;

#[macro_use]
extern crate log;

mod slot;
mod lock;

mod channel;
mod event_loop;
mod mpsc_queue;
mod readiness_stream;
mod tcp;
mod timeout;
mod timer_wheel;
mod udp;

pub mod io;

pub use channel::{Sender, Receiver};
pub use event_loop::{Loop, LoopPin, LoopHandle, AddSource, AddTimeout};
pub use event_loop::{LoopData, AddLoopData, TimeoutToken, IoToken};
pub use readiness_stream::ReadinessStream;
pub use tcp::{TcpListener, TcpStream};
pub use timeout::Timeout;
pub use udp::UdpSocket;
