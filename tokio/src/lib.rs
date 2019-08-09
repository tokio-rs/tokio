#![doc(html_root_url = "https://docs.rs/tokio/0.2.0-alpha.1")]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]
#![feature(async_await)]

//! A runtime for writing reliable, asynchronous, and slim applications.
//!
//! Tokio is an event-driven, non-blocking I/O platform for writing asynchronous
//! applications with the Rust programming language. At a high level, it
//! provides a few major components:
//!
//! * A multi threaded, work-stealing based task [scheduler][runtime].
//! * A [reactor] backed by the operating system's event queue (epoll, kqueue,
//!   IOCP, etc...).
//! * Asynchronous [TCP and UDP][net] sockets.
//! * Asynchronous [filesystem][fs] operations.
//! * [Timer][timer] API for scheduling work in the future.
//!
//! Guide level documentation is found on the [website].
//!
//! [website]: https://tokio.rs/docs/
//!
//! # Examples
//!
//! A simple TCP echo server:
//!
//! ```no_run
//! #![feature(async_await)]
//!
//! use tokio::net::TcpListener;
//! use tokio::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let addr = "127.0.0.1:8080".parse()?;
//!     let mut listener = TcpListener::bind(&addr).unwrap();
//!
//!     loop {
//!         let (mut socket, _) = listener.accept().await?;
//!
//!         tokio::spawn(async move {
//!             let mut buf = [0; 1024];
//!
//!             // In a loop, read data from the socket and write the data back.
//!             loop {
//!                 let n = match socket.read(&mut buf).await {
//!                     // socket closed
//!                     Ok(n) if n == 0 => return,
//!                     Ok(n) => n,
//!                     Err(e) => {
//!                         println!("failed to read from socket; err = {:?}", e);
//!                         return;
//!                     }
//!                 };
//!
//!                 // Write the data back
//!                 if let Err(e) = socket.write_all(&buf[0..n]).await {
//!                     println!("failed to write to socket; err = {:?}", e);
//!                     return;
//!                 }
//!             }
//!         });
//!     }
//! }
//! ```

macro_rules! if_runtime {
    ($($i:item)*) => ($(
        #[cfg(any(feature = "rt-full"))]
        $i
    )*)
}

#[cfg(feature = "timer")]
pub mod clock;
#[cfg(feature = "codec")]
pub mod codec;
#[cfg(feature = "fs")]
pub mod fs;
pub mod future;
#[cfg(feature = "io")]
pub mod io;
#[cfg(any(feature = "tcp", feature = "udp", feature = "uds"))]
pub mod net;
pub mod prelude;
#[cfg(feature = "reactor")]
pub mod reactor;
pub mod stream;
#[cfg(feature = "sync")]
pub mod sync;
#[cfg(feature = "timer")]
pub mod timer;

if_runtime! {
    pub mod executor;
    pub mod runtime;

    #[doc(inline)]
    pub use crate::executor::spawn;

    #[cfg(not(test))] // Work around for rust-lang/rust#62127
    #[doc(inline)]
    pub use tokio_macros::main;
    #[doc(inline)]
    pub use tokio_macros::test;
}
