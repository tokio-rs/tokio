#![doc(html_root_url = "https://docs.rs/tokio/0.2.0-alpha.6")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! A runtime for writing reliable, asynchronous, and slim applications.
//!
//! Tokio is an event-driven, non-blocking I/O platform for writing asynchronous
//! applications with the Rust programming language. At a high level, it
//! provides a few major components:
//!
//! * A multi threaded, work-stealing based task [scheduler][runtime].
//! * A [driver] backed by the operating system's event queue (epoll, kqueue,
//!   IOCP, etc...).
//! * Asynchronous [TCP and UDP][net] sockets.
//! * Asynchronous [filesystem][fs] operations.
//! * [Timer][time] API for scheduling work in the future.
//!
//! Guide level documentation is found on the [website].
//!
//! [driver]: driver/index.html
//! [website]: https://tokio.rs/docs/
//!
//! # Examples
//!
//! A simple TCP echo server:
//!
//! ```no_run
//! use tokio::net::TcpListener;
//! use tokio::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
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

// macros used internally
#[macro_use]
mod macros;

// Blocking task implementation
pub(crate) mod blocking;

cfg_fs! {
    pub mod fs;
}

mod future;

pub mod io;

pub mod net;

mod loom;

pub mod prelude;

cfg_process! {
    pub mod process;
}

pub mod runtime;

cfg_signal! {
    pub mod signal;
}

cfg_sync! {
    pub mod sync;
}
cfg_not_sync! {
    mod sync;
}

cfg_rt_core! {
    pub mod task;
    pub use task::spawn;
}

cfg_time! {
    pub mod time;
}

mod util;

cfg_macros! {
    #[cfg(not(test))] // Work around for rust-lang/rust#62127
    pub use tokio_macros::main;
    pub use tokio_macros::test;
}

// Tests
#[cfg(test)]
mod tests;

// TODO: rm
#[cfg(feature = "io-util")]
#[cfg(test)]
fn is_unpin<T: Unpin>() {}
