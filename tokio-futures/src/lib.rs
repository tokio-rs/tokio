#![doc(html_root_url = "https://docs.rs/tokio-futures/0.2.0")]
#![cfg(feature = "all")]
#![deny(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]

//! Futures

pub mod future;
pub mod sink;
pub mod stream;

mod macros;

pub use crate::future::Future;
pub use crate::sink::Sink;
pub use crate::stream::Stream;
