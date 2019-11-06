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
//! * [Timer][timer] API for scheduling work in the future.
//!
//! Guide level documentation is found on the [website].
//!
//! [driver]: tokio::net::driver
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
macro_rules! if_runtime {
    ($($i:item)*) => ($(
        #[cfg(any(
            feature = "blocking",
            feature = "rt-full",
            feature = "rt-current-thread",
        ))]
        $i
    )*)
}

#[cfg(all(loom, test))]
macro_rules! thread_local {
    ($($tts:tt)+) => { loom::thread_local!{ $($tts)+ } }
}

#[cfg(feature = "timer")]
pub mod clock;

#[cfg(feature = "fs")]
pub mod fs;

pub mod future;

#[cfg(feature = "io-traits")]
pub mod io;

#[cfg(feature = "net-driver")]
pub mod net;

mod loom;

pub mod prelude;

#[cfg(all(feature = "process", not(loom)))]
pub mod process;

pub mod runtime;

#[cfg(feature = "signal")]
#[cfg(not(loom))]
pub mod signal;

pub mod stream;

#[cfg(feature = "sync")]
pub mod sync;

#[cfg(feature = "timer")]
pub mod timer;

#[cfg(feature = "rt-full")]
mod util;

if_runtime! {

    #[doc(inline)]
    pub use crate::runtime::spawn;

    #[cfg(not(test))] // Work around for rust-lang/rust#62127
    #[cfg(feature = "macros")]
    #[doc(inline)]
    pub use tokio_macros::main;

    #[cfg(feature = "macros")]
    #[doc(inline)]
    pub use tokio_macros::test;
}

#[cfg(feature = "io-util")]
#[cfg(test)]
fn is_unpin<T: Unpin>() {}
