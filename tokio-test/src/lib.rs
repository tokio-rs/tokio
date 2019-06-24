#![doc(html_root_url = "https://docs.rs/tokio-test/0.1.0")]
#![deny(
    missing_docs,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! Tokio and Futures based testing utilites
//!
//! # Example
//!
//! ```
//! # use futures::{Future, future};
//! use tokio_test::assert_ready;
//!
//! let mut fut = future::ok::<(), ()>(());
//! assert_ready!(fut.poll());
//! ```

// pub mod clock;
mod macros;
pub mod task;

pub use assertive::{assert_err, assert_ok};

/*
#[doc(hidden)]
pub mod codegen {
    pub mod futures {
        pub use futures::*;
    }
}
*/
