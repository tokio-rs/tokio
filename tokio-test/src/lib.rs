#![doc(html_root_url = "https://docs.rs/tokio-test/0.1.0")]
#![deny(missing_docs, missing_debug_implementations, unreachable_pub)]
#![cfg_attr(test, deny(warnings))]

//! Tokio and Futures based testing utilites

extern crate futures;
extern crate tokio_executor;
extern crate tokio_timer;

pub mod macros;
pub mod task;
pub mod timer;
