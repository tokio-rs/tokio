#![allow(unknown_lints, unexpected_cfgs)]
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
//! [`tokio`]: https://docs.rs/tokio

#[macro_use]
mod cfg;

mod loom;

cfg_codec! {
    #[macro_use]
    mod tracing;

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
