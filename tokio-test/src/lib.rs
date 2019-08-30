#![doc(html_root_url = "https://docs.rs/tokio-test/0.2.0-alpha.4")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! Tokio and Futures based testing utilites

pub mod clock;
pub mod io;
mod macros;
pub mod task;

/// Runs the provided future, blocking the current thread until the
/// future completes.
///
/// For more information, see the documentation for
/// [`tokio::runtime::current_thread::Runtime::block_on`][runtime-block-on].
///
/// [runtime-block-on]: https://docs.rs/tokio/0.2.0-alpha.2/tokio/runtime/current_thread/struct.Runtime.html#method.block_on
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut rt = tokio::runtime::current_thread::Runtime::new().unwrap();
    rt.block_on(future)
}

/*
#[doc(hidden)]
pub mod codegen {
    pub mod futures {
        pub use futures::*;
    }
}
*/
