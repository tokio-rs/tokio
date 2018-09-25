#![feature(arbitrary_self_types, async_await, await_macro, futures_api, pin)]

#![doc(html_root_url = "https://docs.rs/tokio-async-await/0.1.3")]
#![deny(missing_docs, missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]

//! A preview of Tokio w/ `async` / `await` support.

extern crate futures;

/// Extracts the successful type of a `Poll<Result<T, E>>`.
///
/// This macro bakes in propagation of `Pending` and `Err` signals by returning early.
macro_rules! try_ready {
    ($x:expr) => {
        match $x {
            std::task::Poll::Ready(Ok(x)) => x,
            std::task::Poll::Ready(Err(e)) =>
                return std::task::Poll::Ready(Err(e.into())),
            std::task::Poll::Pending =>
                return std::task::Poll::Pending,
        }
    }
}

// Re-export all of Tokio
pub use tokio_main::{
    // Modules
    clock,
    codec,
    executor,
    fs,
    io,
    net,
    reactor,
    runtime,
    timer,
    util,

    // Functions
    run,
    spawn,
};

pub mod sync {
    //! Asynchronous aware synchronization

    pub use tokio_channel::{
        mpsc,
        oneshot,
    };
}

pub mod async_await;

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

use std::future::{Future as StdFuture};

// Rename the `await` macro in `std`
#[doc(hidden)]
pub use std::await as std_await;

/// Like `tokio::run`, but takes an `async` block
pub fn run_async<F>(future: F)
where F: StdFuture<Output = ()> + Send + 'static,
{
    use crate::async_await::compat::backward;

    run(backward::Compat::new(async move {
        let _ = await!(future);
        Ok(())
    }))
}

/// Like `tokio::spawn`, but takes an `async` block
pub fn spawn_async<F>(future: F)
where F: StdFuture<Output = ()> + Send + 'static,
{
    use crate::async_await::compat::backward;

    spawn(backward::Compat::new(async move {
        let _ = await!(future);
        Ok(())
    }));
}
