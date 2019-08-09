#![doc(html_root_url = "https://docs.rs/tokio-test/0.2.0-alpha.1")]
#![warn(
    missing_docs,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! Tokio and Futures based testing utilites

pub mod clock;
pub mod io;
mod macros;
pub mod task;

/*
#[doc(hidden)]
pub mod codegen {
    pub mod futures {
        pub use futures::*;
    }
}
*/
