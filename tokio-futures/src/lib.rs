#![cfg(feature = "async-await-preview")]
#![feature(async_await, await_macro)]
#![doc(html_root_url = "https://docs.rs/tokio-futures/0.1.0")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! A preview of Tokio w/ `async` / `await` support.

/// Extracts the successful type of a `Poll<Result<T, E>>`.
///
/// This macro bakes in propagation of `Pending` and `Err` signals by returning early.
macro_rules! try_ready {
    ($x:expr) => {
        match $x {
            std::task::Poll::Ready(Ok(x)) => x,
            std::task::Poll::Ready(Err(e)) => return std::task::Poll::Ready(Err(e.into())),
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    };
}

#[macro_use]
mod async_wait;
pub mod compat;
pub mod io;
pub mod sink;
pub mod stream;
