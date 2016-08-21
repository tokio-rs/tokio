//! Mio bindings with streams and futures
//!
//! This crate uses the `futures_io` and `futures` crates to provide a thin
//! binding on top of mio of TCP and UDP sockets.

#![deny(missing_docs)]

extern crate futures;
extern crate futures_io;
extern crate mio;
extern crate slab;

#[macro_use]
extern crate scoped_tls;

#[macro_use]
extern crate log;

mod readiness_stream;
mod event_loop;
mod tcp;
mod udp;
mod timeout;
mod timer_wheel;
#[path = "../../src/slot.rs"]
mod slot;
#[path = "../../src/lock.rs"]
mod lock;
mod mpsc_queue;
mod channel;

pub use event_loop::{Loop, LoopPin, LoopHandle, AddSource, AddTimeout};
pub use event_loop::{LoopData, AddLoopData, TimeoutToken, IoToken};
pub use readiness_stream::ReadinessStream;
pub use tcp::{TcpListener, TcpStream};
pub use timeout::Timeout;
pub use udp::UdpSocket;
