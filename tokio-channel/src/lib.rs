#![doc(html_root_url = "https://docs.rs/tokio-channel/0.1.0")]
#![deny(missing_docs, warnings, missing_debug_implementations)]

//! Asynchronous channels.
//!
//! This crate provides channels that can be used to communicate between
//! asynchronous tasks.

extern crate futures;

pub mod mpsc;
pub mod oneshot;

mod lock;
