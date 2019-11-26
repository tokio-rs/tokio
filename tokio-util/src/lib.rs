#![doc(html_root_url = "https://docs.rs/tokio-util/0.2.0")]
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
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Utilities for working with Tokio.

#[macro_use]
mod cfg;

cfg_codec! {
    pub mod codec;
}

cfg_udp! {
    pub mod udp;
}
