#![doc(html_root_url = "https://docs.rs/tokio-test/0.1.0")]
#![deny(
    missing_docs,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![cfg_attr(test, deny(warnings))]

//! Tokio and Futures based testing utilites
//!
//! # Example
//!
//! ```
//! # extern crate futures;
//! # #[macro_use] extern crate tokio_test;
//! # use futures::{Future, future};
//! let mut fut = future::ok::<(), ()>(());
//! assert_ready!(fut.poll());
//! ```

pub mod clock;
mod macros;
pub mod task;

#[doc(hidden)]
pub mod codegen {
    pub mod futures {
        pub use futures::*;
    }
}
