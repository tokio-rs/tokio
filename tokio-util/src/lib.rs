#![doc(html_root_url = "https://docs.rs/tokio-util/0.4.0")]
#![allow(clippy::needless_doctest_main)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![cfg_attr(docsrs, deny(broken_intra_doc_links))]
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
    pub mod codec;
}

/*
Disabled due to removal of poll_ functions on UdpSocket.

See https://github.com/tokio-rs/tokio/issues/2830
cfg_udp! {
    pub mod udp;
}
*/

cfg_compat! {
    pub mod compat;
}

cfg_io! {
    pub mod io;
}

pub mod context;

pub mod sync;

pub mod either;
