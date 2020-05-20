//! The [syscall] module is intended to provide a centralized location
//! for interacting with OS resources such as disks and network.
//!
//! This requires both the `"test-util"` feature to be enabled as well
//! as the `--cfg tokio_unstable` flag to be supplied.
//!
//! ## Extension
//! The Syscall trait allows hooking into implementations of Tokio
//! disk and networking resources to supply alternate implementations
//! or mocks.
//!
//! Extension requires compiling with `--cfg tokio_unstable` in addition
//! to the `syscall` feature flag.
//!
//! [syscall]:crate::syscall
mod default;
use std::future::Future;

use std::io;
use std::pin::Pin;

/// Syscalls trait allows for hooking into the Tokio runtime.
pub trait Syscalls: Send + Sync {
    /// Spawn a Future onto the runtime.
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()>>>);

    /// Spawn a blocking task onto the runtime.
    fn spawn_blocking(&self, task: Box<dyn FnOnce()>);

    /// Drive the runtime forward
    fn park(&self);

    /// Drive the runtime forward with a timeout.
    fn park_timeout(&self, duration: std::time::Duration);

    /// Unblock the runtime
    fn unpark(&self);
}
