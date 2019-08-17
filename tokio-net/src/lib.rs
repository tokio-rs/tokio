#![doc(html_root_url = "https://docs.rs/tokio-net/0.2.0-alpha.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]
#![feature(async_await)]

//! Event loop that drives Tokio I/O resources.
//!
//! The reactor is the engine that drives asynchronous I/O resources (like TCP and
//! UDP sockets). It is backed by [`mio`] and acts as a bridge between [`mio`] and
//! [`futures`].
//!
//! The crate provides:
//!
//! * [`Reactor`] is the main type of this crate. It performs the event loop logic.
//!
//! * [`Handle`] provides a reference to a reactor instance.
//!
//! * [`Registration`] and [`PollEvented`] allow third parties to implement I/O
//!   resources that are driven by the reactor.
//!
//! Application authors will not use this crate directly. Instead, they will use the
//! `tokio` crate. Library authors should only depend on `tokio-net` if they
//! are building a custom I/O resource.
//!
//! For more details, see [reactor module] documentation in the Tokio crate.
//!
//! [`mio`]: http://github.com/carllerche/mio
//! [`futures`]: http://github.com/rust-lang-nursery/futures-rs
//! [`Reactor`]: struct.Reactor.html
//! [`Handle`]: struct.Handle.html
//! [`Registration`]: struct.Registration.html
//! [`PollEvented`]: struct.PollEvented.html
//! [reactor module]: https://docs.rs/tokio/0.1/tokio/reactor/index.html

pub mod driver;
pub mod util;

#[cfg(feature = "signal")]
pub mod signal;

#[cfg(feature = "tcp")]
pub mod tcp;

#[cfg(feature = "udp")]
pub mod udp;

#[cfg(feature = "uds")]
#[cfg(unix)]
pub mod uds;
