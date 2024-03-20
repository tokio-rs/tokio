#![allow(clippy::needless_doctest_main)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Utilities for working with Tokio.
//!
//! This crate is not versioned in lockstep with the core
//! [`tokio`] crate. However, `tokio-util` _will_ respect Rust's
//! semantic versioning policy, especially with regard to breaking changes.
//!
//! # Cancellation safety
//!
//! When using `tokio::select!` in a loop to receive messages from multiple sources,
//! you should make sure that the receive call is cancellation safe to avoid
//! losing messages. This section goes through various common methods and
//! describes whether they are cancel safe.  The lists in this section are not
//! exhaustive.
//!
//! * [`futures_util::sink::SinkExt::send`]: if send is used as the event in a
//! `tokio::select!` statement and some other branch completes first, then it is
//! guaranteed that the message was not sent, but the message itself is lost.
//!
//! [`tokio`]: https://docs.rs/tokio
//! [`futures_util::sink::SinkExt::send`]: https://docs.rs/futures-util/latest/futures_util/sink/trait.SinkExt.html#method.send

#[macro_use]
mod cfg;

mod loom;

cfg_codec! {
    pub mod codec;
}

cfg_net! {
    #[cfg(not(target_arch = "wasm32"))]
    pub mod udp;
    pub mod net;
}

cfg_compat! {
    pub mod compat;
}

cfg_io! {
    pub mod io;
}

cfg_rt! {
    pub mod context;
    pub mod task;
}

cfg_time! {
    pub mod time;
}

pub mod sync;

pub mod either;

pub use bytes;

mod util;
