//! Tokio-uring provides a safe [io-uring] interface for the Tokio runtime. The
//! library requires Linux kernel 5.10 or later.
//!
//! [io-uring]: https://kernel.dk/io_uring.pdf
//!
//! # Getting started
//!
//! Using `tokio-uring` requires starting a [`tokio-uring`] runtime. This
//! runtime internally manages the main Tokio runtime and a `io-uring` driver.
//!
//! ```no_run
//! use tokio::platform::linux::uring::fs::File;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!         // Open a file
//!         let file = File::open("hello.txt").await?;
//!
//!         let buf = vec![0; 4096];
//!         // Read some data, the buffer is passed by ownership and
//!         // submitted to the kernel. When the operation completes,
//!         // we get the buffer back.
//!         let (res, buf) = file.read_at(buf, 0).await;
//!         let n = res?;
//!
//!         // Display the contents
//!         println!("{:?}", &buf[..n]);
//!
//!         Ok(())
//! }
//! ```
//!
//! Under the hood, `tokio_uring::start` starts a [`current-thread`] Runtime.
//! For concurrency, spawn multiple threads, each with a `tokio-uring` runtime.
//! The `tokio-uring` resource types are optimized for single-threaded usage and
//! most are `!Sync`.
//!
//! # Submit-based operations
//!
//! Unlike Tokio proper, `io-uring` is based on submission based operations.
//! Ownership of resources are passed to the kernel, which then performs the
//! operation. When the operation completes, ownership is passed back to the
//! caller. Because of this difference, the `tokio-uring` APIs diverge.
//!
//! For example, in the above example, reading from a `File` requires passing
//! ownership of the buffer.
//!
//! # Closing resources
//!
//! With `io-uring`, closing a resource (e.g. a file) is an asynchronous
//! operation. Because Rust does not support asynchronous drop yet, resource
//! types provide an explicit `close()` function. If the `close()` function is
//! not called, the resource will still be closed on drop, but the operation
//! will happen in the background. There is no guarantee as to **when** the
//! implicit close-on-drop operation happens, so it is recommended to explicitly
//! call `close()`.

#![warn(missing_docs)]

macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

#[macro_use]
mod future;
pub(crate) mod driver;

pub mod buf;
pub mod fs;
pub mod net;

/// A specialized `Result` type for `io-uring` operations with buffers.
///
/// This type is used as a return value for asynchronous `io-uring` methods that
/// require passing ownership of a buffer to the runtime. When the operation
/// completes, the buffer is returned whether or not the operation completed
/// successfully.
///
/// # Examples
///
/// ```no_run
/// use tokio::platform::linux::uring::fs::File;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///         // Open a file
///         let file = File::open("hello.txt").await?;
///
///         let buf = vec![0; 4096];
///         // Read some data, the buffer is passed by ownership and
///         // submitted to the kernel. When the operation completes,
///         // we get the buffer back.
///         let (res, buf) = file.read_at(buf, 0).await;
///         let n = res?;
///
///         // Display the contents
///         println!("{:?}", &buf[..n]);
///
///         Ok(())
/// }
/// ```
pub type BufResult<T, B> = (std::io::Result<T>, B);
