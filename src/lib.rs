//! A binding to mio giving it a future/stream interface on top.
//!
//! This library contains the rudimentary bindings to an event loop in mio which
//! provides future and stream-based abstractions of all the underlying I/O
//! objects that mio provides internally.
//!
//! Currently very much a work in progress, and breakage should be expected!

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
#[path = "../../src/slot.rs"]
mod slot;
#[path = "../../src/lock.rs"]
mod lock;

pub use event_loop::{Loop, LoopHandle};
pub use readiness_stream::ReadinessStream;
pub use tcp::{TcpListener, TcpStream};
pub use udp::UdpSocket;
