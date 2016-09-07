//! `Future`-powered I/O at the core of Tokio
//!
//! This crate uses the `futures` crate to provide an event loop ("reactor
//! core") which can be used to drive I/O like TCP and UDP, spawned future
//! tasks, and other events like channels/timeouts. All asynchronous I/O is
//! powered by the `mio` crate.
//!
//! The concrete types provided in this crate are relatively bare bones but are
//! intended to be the essential foundation for further projects needing an
//! event loop. In this crate you'll find:
//!
//! * TCP, both streams and listeners
//! * UDP sockets
//! * Message queues
//! * Timeouts
//!
//! More functionality is likely to be added over time, but otherwise the crate
//! is intended to be flexible with the `PollEvented` type which accepts any
//! type which implements `mio::Evented`. Using this if you'd like Unix domain
//! sockets, for example, the `tokio-uds` is built externally to offer this
//! functionality.
//!
//! Some other important tasks covered by this crate are:
//!
//! * The ability to spawn futures into an even loop. The `Handle` and `Pinned`
//!   types have a `spawn` method which allows executing a future on an event
//!   loop. The `Pinned::spawn` method crucially does not require the future
//!   itself to be `Send`.
//!
//! * The `Io` trait serves as an abstraction for future crates to build on top
//!   of. This packages up `Read` and `Write` functionality as well as the
//!   ability to poll for readiness on both ends.
//!
//! * All I/O is futures-aware. If any action in this crate returns "not ready"
//!   or "would block", then the current future task is scheduled to receive a
//!   notification when it would otherwise make progress.
//!
//! # Examples
//!
//! A simple TCP echo server:
//!
//! ```no_run
//! extern crate futures;
//! extern crate tokio_core;
//!
//! use std::env;
//! use std::net::SocketAddr;
//!
//! use futures::Future;
//! use futures::stream::Stream;
//! use tokio_core::io::{copy, Io};
//! use tokio_core::net::TcpListener;
//! use tokio_core::reactor::Core;
//!
//! fn main() {
//!     let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
//!     let addr = addr.parse::<SocketAddr>().unwrap();
//!
//!     // Create the event loop that will drive this server
//!     let mut l = Core::new().unwrap();
//!     let pin = l.pin();
//!
//!     // Create a TCP listener which will listen for incoming connections
//!     let server = TcpListener::bind(&addr, pin.handle());
//!
//!     let done = server.and_then(|socket| {
//!         // Once we've got the TCP listener, inform that we have it
//!         println!("Listening on: {}", addr);
//!
//!         // Pull out the stream of incoming connections and then for each new
//!         // one spin up a new task copying data.
//!         //
//!         // We use the `io::copy` future to copy all data from the
//!         // reading half onto the writing half.
//!         socket.incoming().for_each(|(socket, addr)| {
//!             let pair = futures::lazy(|| Ok(socket.split()));
//!             let amt = pair.and_then(|(reader, writer)| copy(reader, writer));
//!
//!             // Once all that is done we print out how much we wrote, and then
//!             // critically we *spawn* this future which allows it to run
//!             // concurrently with other connections.
//!             pin.spawn(amt.then(move |result| {
//!                 println!("wrote {:?} bytes to {}", result, addr);
//!                 Ok(())
//!             }));
//!
//!             Ok(())
//!         })
//!     });
//!
//!     // Execute our server (modeled as a future) and wait for it to
//!     // complete.
//!     l.run(done).unwrap();
//! }
//! ```

#![deny(missing_docs)]

#[macro_use]
extern crate futures;
extern crate mio;
extern crate slab;

#[macro_use]
extern crate scoped_tls;

#[macro_use]
extern crate log;

mod slot;
mod lock;

#[macro_use]
pub mod io;

mod mpsc_queue;
mod timer_wheel;
pub mod channel;
pub mod net;
pub mod reactor;
