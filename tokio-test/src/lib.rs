#![doc(html_root_url = "https://docs.rs/tokio-test/0.4.0")]
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

//! Tokio and Futures based testing utilites

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
    use tokio::runtime;

    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(future)
}
