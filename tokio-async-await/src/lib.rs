#![feature(futures_api, await_macro, pin, arbitrary_self_types)]

#![doc(html_root_url = "https://docs.rs/tokio-async-await/0.1.3")]
#![deny(missing_docs, missing_debug_implementations)]
#![cfg_attr(test, deny(warnings))]

//! A preview of Tokio w/ `async` / `await` support.

extern crate futures;
extern crate futures_core;
extern crate futures_util;

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

use futures_core::{
    Future as Future03,
};

// Rename the `await` macro in `std`
#[doc(hidden)]
pub use std::await as std_await;

/// Like `tokio::run`, but takes an `async` block
pub fn run_async<F>(future: F)
where F: Future03<Output = ()> + Send + 'static,
{
    use futures_util::future::FutureExt;
    use crate::async_await::compat::backward;

    let future = future.map(|_| Ok(()));
    run(backward::Compat::new(future))
}

/// Like `tokio::spawn`, but takes an `async` block
pub fn spawn_async<F>(future: F)
where F: Future03<Output = ()> + Send + 'static,
{
    use futures_util::future::FutureExt;
    use crate::async_await::compat::backward;

    let future = future.map(|_| Ok(()));
    spawn(backward::Compat::new(future));
}
