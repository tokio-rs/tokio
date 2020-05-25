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
pub(crate) use default::DefaultSyscalls;
use std::io;
use std::future::Future;
use std::pin::Pin;

/// Syscalls trait allows for hooking into the Tokio runtime.
pub trait Syscalls: Send + Sync {
    /// Drive the runtime forward.
    fn park(&self) -> io::Result<()>;

    /// Drive the runtime forward with a timeout.
    fn park_timeout(&self, duration: std::time::Duration) -> io::Result<()>;

    /// Unpark the runtime.
    fn unpark(&self);

    /// Wrap all spawned futures.
    fn wrap_future(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        future
    }
}
