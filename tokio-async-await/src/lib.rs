#![cfg(feature = "async-await-preview")]
#![feature(rust_2018_preview, async_await, await_macro, futures_api)]
#![doc(html_root_url = "https://docs.rs/tokio-async-await/0.1.5")]
#![deny(missing_docs, missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]

//! A preview of Tokio w/ `async` / `await` support.

extern crate futures;
extern crate tokio_io;

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
mod await;
pub mod compat;
pub mod io;
pub mod sink;
pub mod stream;

/*
pub mod prelude {
    //! A "prelude" for users of the `tokio` crate.
    //!
    //! This prelude is similar to the standard library's prelude in that you'll
    //! almost always want to import its entire contents, but unlike the standard
    //! library's prelude you'll have to do so manually:
    //!
    //! ```
    //! use tokio::prelude::*;
    //! ```
    //!
    //! The prelude may grow over time as additional items see ubiquitous use.

    pub use tokio_main::prelude::*;

    #[doc(inline)]
    pub use crate::async_await::{
        io::{
            AsyncReadExt,
            AsyncWriteExt,
        },
        sink::{
            SinkExt,
        },
        stream::{
            StreamExt,
        },
    };
}
*/

// Rename the `await` macro in `std`. This is used by the redefined
// `await` macro in this crate.
#[doc(hidden)]
pub use std::await as std_await;

/*
use std::future::{Future as StdFuture};

fn run<T: futures::Future<Item = (), Error = ()>>(t: T) {
    drop(t);
}

async fn map_ok<T: StdFuture>(future: T) -> Result<(), ()> {
    let _ = await!(future);
    Ok(())
}

/// Like `tokio::run`, but takes an `async` block
pub fn run_async<F>(future: F)
where F: StdFuture<Output = ()> + Send + 'static,
{
    use async_await::compat::backward;
    let future = backward::Compat::new(map_ok(future));

    run(future);
    unimplemented!();
}
*/

/*
/// Like `tokio::spawn`, but takes an `async` block
pub fn spawn_async<F>(future: F)
where F: StdFuture<Output = ()> + Send + 'static,
{
    use crate::async_await::compat::backward;

    spawn(backward::Compat::new(async || {
        let _ = await!(future);
        Ok(())
    }));
}
*/
