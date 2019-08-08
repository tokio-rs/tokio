#![doc(html_root_url = "https://docs.rs/tokio-process/0.3.0-alpha.1")]

//! An implementation of asynchronous process management for Tokio.
//!
//! This crate provides a `CommandExt` trait to enhance the functionality of the
//! `Command` type in the standard library. The three methods provided by this
//! trait mirror the "spawning" methods in the standard library. The
//! `CommandExt` trait in this crate, though, returns "future aware" types that
//! interoperate with Tokio. The asynchronous process support is provided
//! through signal handling on Unix and system APIs on Windows.
//!
//! # Examples
//!
//! Here's an example program which will spawn `echo hello world` and then wait
//! for it complete.
//!
//! ```no_run
//! #![feature(async_await)]
//!
//! use std::process::Command;
//! use tokio_process::CommandExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Use the standard library's `Command` type to build a process and
//!     // then execute it via the `CommandExt` trait.
//!     let child = Command::new("echo").arg("hello").arg("world")
//!                         .spawn_async();
//!
//!     // Make sure our child succeeded in spawning and process the result
//!     let future = child.expect("failed to spawn");
//!
//!     // Await until the future (and the command) completes
//!     let status = future.await?;
//!     println!("the command exited with: {}", status);
//!     Ok(())
//! }
//! ```
//!
//! Next, let's take a look at an example where we not only spawn `echo hello
//! world` but we also capture its output.
//!
//! ```no_run
//! #![feature(async_await)]
//!
//! use std::process::Command;
//! use tokio_process::CommandExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Like above, but use `output_async` which returns a future instead of
//!     // immediately returning the `Child`.
//!     let output = Command::new("echo").arg("hello").arg("world")
//!                         .output_async();
//!
//!     let output = output.await?;
//!
//!     assert!(output.status.success());
//!     assert_eq!(output.stdout, b"hello world\n");
//!     Ok(())
//! }
//! ```
//!
//! We can also read input line by line.
//!
//! ```no_run
//! #![feature(async_await)]
//!
//! use futures_util::stream::StreamExt;
//! use std::process::{Command, Stdio};
//! use tokio::codec::{FramedRead, LinesCodec};
//! use tokio_process::CommandExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut cmd = Command::new("cat");
//!
//!     // Specify that we want the command's standard output piped back to us.
//!     // By default, standard input/output/error will be inherited from the
//!     // current process (for example, this means that standard input will
//!     // come from the keyboard and standard output/error will go directly to
//!     // the terminal if this process is invoked from the command line).
//!     cmd.stdout(Stdio::piped());
//!
//!     let mut child = cmd.spawn_async()
//!         .expect("failed to spawn command");
//!
//!     let stdout = child.stdout().take()
//!         .expect("child did not have a handle to stdout");
//!
//!     let mut reader = FramedRead::new(stdout, LinesCodec::new());
//!
//!     // Ensure the child process is spawned in the runtime so it can
//!     // make progress on its own while we await for any output.
//!     tokio::spawn(async {
//!         let status = child.await
//!             .expect("child process encountered an error");
//!
//!         println!("child status was: {}", status);
//!     });
//!
//!     while let Some(line) = reader.next().await {
//!         println!("Line: {}", line?);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # Caveats
//!
//! While similar to the standard library, this crate's `Child` type differs
//! importantly in the behavior of `drop`. In the standard library, a child
//! process will continue running after the instance of `std::process::Child`
//! is dropped. In this crate, however, because `tokio_process::Child` is a
//! future of the child's `ExitStatus`, a child process is terminated if
//! `tokio_process::Child` is dropped. The behavior of the standard library can
//! be regained with the `Child::forget` method.

#![doc(html_root_url = "https://docs.rs/tokio-process/0.3.0")]
#![deny(missing_debug_implementations, missing_docs, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]
#![feature(async_await)]

#[cfg(unix)]
#[macro_use]
extern crate lazy_static;
#[cfg(unix)]
#[macro_use]
extern crate log;

use std::io::{self, Read, Write};
use std::process::{Command, ExitStatus, Output, Stdio};

use futures_core::future::TryFuture;
use futures_util::future;
use futures_util::future::FutureExt;
use futures_util::try_future::TryFutureExt;

use kill::Kill;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use tokio_io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio_reactor::Handle;

#[path = "unix/mod.rs"]
#[cfg(unix)]
mod imp;

#[path = "windows.rs"]
#[cfg(windows)]
mod imp;

mod kill;

/// Extensions provided by this crate to the `Command` type in the standard
/// library.
///
/// This crate primarily enhances the standard library's `Command` type with
/// asynchronous capabilities. The currently three blocking functions in the
/// standard library, `spawn`, `status`, and `output`, all have asynchronous
/// versions through this trait.
///
/// Note that the `Child` type spawned is specific to this crate, and that the
/// I/O handles created from this crate are all asynchronous as well (differing
/// from their `std` counterparts).
pub trait CommandExt {
    /// Executes the command as a child process, returning a handle to it.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    ///
    /// This method will spawn the child process synchronously and return a
    /// handle to a future-aware child process. The `Child` returned implements
    /// `Future` itself to acquire the `ExitStatus` of the child, and otherwise
    /// the `Child` has methods to acquire handles to the stdin, stdout, and
    /// stderr streams.
    ///
    /// All I/O this child does will be associated with the current default
    /// event loop.
    fn spawn_async(&mut self) -> io::Result<Child> {
        self.spawn_async_with_handle(&Handle::default())
    }

    /// Executes the command as a child process, returning a handle to it.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    ///
    /// This method will spawn the child process synchronously and return a
    /// handle to a future-aware child process. The `Child` returned implements
    /// `Future` itself to acquire the `ExitStatus` of the child, and otherwise
    /// the `Child` has methods to acquire handles to the stdin, stdout, and
    /// stderr streams.
    ///
    /// The `handle` specified to this method must be a handle to a valid event
    /// loop, and all I/O this child does will be associated with the specified
    /// event loop.
    fn spawn_async_with_handle(&mut self, handle: &Handle) -> io::Result<Child>;

    /// Executes a command as a child process, waiting for it to finish and
    /// collecting its exit status.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    ///
    /// The `StatusAsync` future returned will resolve to the `ExitStatus`
    /// type in the standard library representing how the process exited. If
    /// any input/output handles are set to a pipe then they will be immediately
    ///  closed after the child is spawned.
    ///
    /// All I/O this child does will be associated with the current default
    /// event loop.
    ///
    /// If the `StatusAsync` future is dropped before the future resolves, then
    /// the child will be killed, if it was spawned.
    ///
    /// # Errors
    ///
    /// This function will return an error immediately if the child process
    /// cannot be spawned. Otherwise errors obtained while waiting for the child
    /// are returned through the `StatusAsync` future.
    fn status_async(&mut self) -> io::Result<StatusAsync> {
        self.status_async_with_handle(&Handle::default())
    }

    /// Executes a command as a child process, waiting for it to finish and
    /// collecting its exit status.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    ///
    /// The `StatusAsync` future returned will resolve to the `ExitStatus`
    /// type in the standard library representing how the process exited. If
    /// any input/output handles are set to a pipe then they will be immediately
    ///  closed after the child is spawned.
    ///
    /// The `handle` specified must be a handle to a valid event loop, and all
    /// I/O this child does will be associated with the specified event loop.
    ///
    /// If the `StatusAsync` future is dropped before the future resolves, then
    /// the child will be killed, if it was spawned.
    ///
    /// # Errors
    ///
    /// This function will return an error immediately if the child process
    /// cannot be spawned. Otherwise errors obtained while waiting for the child
    /// are returned through the `StatusAsync` future.
    fn status_async_with_handle(&mut self, handle: &Handle) -> io::Result<StatusAsync>;

    /// Executes the command as a child process, waiting for it to finish and
    /// collecting all of its output.
    ///
    /// > **Note**: this method, unlike the standard library, will
    /// > unconditionally configure the stdout/stderr handles to be pipes, even
    /// > if they have been previously configured. If this is not desired then
    /// > the `spawn_async` method should be used in combination with the
    /// > `wait_with_output` method on child.
    ///
    /// This method will return a future representing the collection of the
    /// child process's stdout/stderr. The `OutputAsync` future will resolve to
    /// the `Output` type in the standard library, containing `stdout` and
    /// `stderr` as `Vec<u8>` along with an `ExitStatus` representing how the
    /// process exited.
    ///
    /// All I/O this child does will be associated with the current default
    /// event loop.
    ///
    /// If the `OutputAsync` future is dropped before the future resolves, then
    /// the child will be killed, if it was spawned.
    fn output_async(&mut self) -> OutputAsync {
        self.output_async_with_handle(&Handle::default())
    }

    /// Executes the command as a child process, waiting for it to finish and
    /// collecting all of its output.
    ///
    /// > **Note**: this method, unlike the standard library, will
    /// > unconditionally configure the stdout/stderr handles to be pipes, even
    /// > if they have been previously configured. If this is not desired then
    /// > the `spawn_async` method should be used in combination with the
    /// > `wait_with_output` method on child.
    ///
    /// This method will return a future representing the collection of the
    /// child process's stdout/stderr. The `OutputAsync` future will resolve to
    /// the `Output` type in the standard library, containing `stdout` and
    /// `stderr` as `Vec<u8>` along with an `ExitStatus` representing how the
    /// process exited.
    ///
    /// The `handle` specified must be a handle to a valid event loop, and all
    /// I/O this child does will be associated with the specified event loop.
    ///
    /// If the `OutputAsync` future is dropped before the future resolves, then
    /// the child will be killed, if it was spawned.
    fn output_async_with_handle(&mut self, handle: &Handle) -> OutputAsync;
}

struct SpawnedChild {
    child: imp::Child,
    stdin: Option<imp::ChildStdin>,
    stdout: Option<imp::ChildStdout>,
    stderr: Option<imp::ChildStderr>,
}

impl CommandExt for Command {
    fn spawn_async_with_handle(&mut self, handle: &Handle) -> io::Result<Child> {
        imp::spawn_child(self, handle).map(|spawned_child| Child {
            child: ChildDropGuard::new(spawned_child.child),
            stdin: spawned_child.stdin.map(|inner| ChildStdin { inner }),
            stdout: spawned_child.stdout.map(|inner| ChildStdout { inner }),
            stderr: spawned_child.stderr.map(|inner| ChildStderr { inner }),
        })
    }

    fn status_async_with_handle(&mut self, handle: &Handle) -> io::Result<StatusAsync> {
        self.spawn_async_with_handle(handle).map(|mut child| {
            // Ensure we close any stdio handles so we can't deadlock
            // waiting on the child which may be waiting to read/write
            // to a pipe we're holding.
            child.stdin.take();
            child.stdout.take();
            child.stderr.take();

            StatusAsync { inner: child }
        })
    }

    fn output_async_with_handle(&mut self, handle: &Handle) -> OutputAsync {
        self.stdout(Stdio::piped());
        self.stderr(Stdio::piped());

        let inner =
            future::ready(self.spawn_async_with_handle(handle)).and_then(Child::wait_with_output);

        OutputAsync {
            inner: inner.boxed(),
        }
    }
}

/// A drop guard which ensures the child process is killed on drop to maintain
/// the contract of dropping a Future leads to "cancellation".
#[derive(Debug)]
struct ChildDropGuard<T: Kill> {
    inner: T,
    kill_on_drop: bool,
}

impl<T: Kill> ChildDropGuard<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            kill_on_drop: true,
        }
    }

    fn forget(&mut self) {
        self.kill_on_drop = false;
    }
}

impl<T: Kill> Kill for ChildDropGuard<T> {
    fn kill(&mut self) -> io::Result<()> {
        let ret = self.inner.kill();

        if ret.is_ok() {
            self.kill_on_drop = false;
        }

        ret
    }
}

impl<T: Kill> Drop for ChildDropGuard<T> {
    fn drop(&mut self) {
        if self.kill_on_drop {
            drop(self.kill());
        }
    }
}

impl<T: TryFuture + Kill + Unpin> Future for ChildDropGuard<T> {
    type Output = Result<T::Ok, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let ret = Pin::new(&mut self.inner).try_poll(cx);

        if let Poll::Ready(Ok(_)) = ret {
            // Avoid the overhead of trying to kill a reaped process
            self.kill_on_drop = false;
        }

        ret
    }
}

/// Representation of a child process spawned onto an event loop.
///
/// This type is also a future which will yield the `ExitStatus` of the
/// underlying child process. A `Child` here also provides access to information
/// like the OS-assigned identifier and the stdio streams.
///
/// > **Note**: The behavior of `drop` on a child in this crate is *different
/// > than the behavior of the standard library*. If a `tokio_process::Child` is
/// > dropped before the process finishes then the process will be terminated.
/// > In the standard library, however, the process continues executing. This is
/// > done because futures in general take `drop` as a sign of cancellation, and
/// > this `Child` is itself a future. If you'd like to run a process in the
/// > background, though, you may use the `forget` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Child {
    child: ChildDropGuard<imp::Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
}

impl Child {
    /// Returns the OS-assigned process identifier associated with this child.
    pub fn id(&self) -> u32 {
        self.child.inner.id()
    }

    /// Forces the child to exit.
    ///
    /// This is equivalent to sending a SIGKILL on unix platforms.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    /// Returns a handle for writing to the child's stdin, if it has been
    /// captured
    pub fn stdin(&mut self) -> &mut Option<ChildStdin> {
        &mut self.stdin
    }

    /// Returns a handle for writing to the child's stdout, if it has been
    /// captured
    pub fn stdout(&mut self) -> &mut Option<ChildStdout> {
        &mut self.stdout
    }

    /// Returns a handle for writing to the child's stderr, if it has been
    /// captured
    pub fn stderr(&mut self) -> &mut Option<ChildStderr> {
        &mut self.stderr
    }

    /// Returns a future that will resolve to an `Output`, containing the exit
    /// status, stdout, and stderr of the child process.
    ///
    /// The returned future will simultaneously waits for the child to exit and
    /// collect all remaining output on the stdout/stderr handles, returning an
    /// `Output` instance.
    ///
    /// The stdin handle to the child process, if any, will be closed before
    /// waiting. This helps avoid deadlock: it ensures that the child does not
    /// block waiting for input from the parent, while the parent waits for the
    /// child to exit.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent. In
    /// order to capture the output into this `Output` it is necessary to create
    /// new pipes between parent and child. Use `stdout(Stdio::piped())` or
    /// `stderr(Stdio::piped())`, respectively, when creating a `Command`.
    pub fn wait_with_output(mut self) -> WaitWithOutput {
        drop(self.stdin().take());
        let stdout_val = self.stdout.take();
        let stderr_val = self.stderr.take();
        let stdout_fut = async {
            match stdout_val {
                Some(mut io) => {
                    let mut vec = Vec::new();
                    AsyncReadExt::read_to_end(&mut io, &mut vec).await?;
                    Ok(vec)
                }
                None => Ok(Vec::new()),
            }
        };
        let stderr_fut = async {
            match stderr_val {
                Some(mut io) => {
                    let mut vec = Vec::new();
                    AsyncReadExt::read_to_end(&mut io, &mut vec).await?;
                    Ok(vec)
                }
                None => Ok(Vec::new()),
            }
        };

        WaitWithOutput {
            inner: futures_util::try_future::try_join3(stdout_fut, stderr_fut, self)
                .and_then(|(stdout, stderr, status)| {
                    future::ok(Output {
                        status,
                        stdout,
                        stderr,
                    })
                })
                .boxed(),
        }
    }

    /// Drop this `Child` without killing the underlying process.
    ///
    /// Normally a `Child` is killed if it's still alive when dropped, but this
    /// method will ensure that the child may continue running once the `Child`
    /// instance is dropped.
    ///
    /// > **Note**: this method may leak OS resources depending on your platform.
    /// > To ensure resources are eventually cleaned up, consider sending the
    /// > `Child` instance into an event loop as an alternative to this method.
    ///
    /// ```no_run
    /// # #![feature(async_await)]
    /// # use std::process::Command;
    /// # use tokio_process::CommandExt;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let child = Command::new("echo").arg("hello").arg("world")
    ///                     .spawn_async()
    ///                     .expect("failed to spawn");
    ///
    /// tokio::spawn(async {
    ///   let _ = child.await;
    /// });
    /// # }
    pub fn forget(mut self) {
        self.child.forget();
    }
}

impl Future for Child {
    type Output = io::Result<ExitStatus>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.child).poll(cx)
    }
}

/// Future returned from the `Child::wait_with_output` method.
///
/// This future will resolve to the standard library's `Output` type which
/// contains the exit status, stdout, and stderr of a child process.
#[must_use = "futures do nothing unless polled"]
pub struct WaitWithOutput {
    inner: Pin<Box<dyn Future<Output = io::Result<Output>> + Send>>,
}

impl fmt::Debug for WaitWithOutput {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("WaitWithOutput")
            .field("inner", &"..")
            .finish()
    }
}

impl Future for WaitWithOutput {
    type Output = io::Result<Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx)
    }
}

/// Future returned by the `CommandExt::status_async` method.
///
/// This future is used to conveniently spawn a child and simply wait for its
/// exit status. This future will resolves to the `ExitStatus` type in the
/// standard library.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct StatusAsync {
    inner: Child,
}

impl Future for StatusAsync {
    type Output = io::Result<ExitStatus>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx)
    }
}

/// Future returned by the `CommandExt::output_async` method.
///
/// This future is mostly equivalent to spawning a process and then calling
/// `wait_with_output` on it internally. This can be useful to simply spawn a
/// process, collecting all of its output and its exit status.
#[must_use = "futures do nothing unless polled"]
pub struct OutputAsync {
    inner: Pin<Box<dyn Future<Output = io::Result<Output>> + Send>>,
}

impl fmt::Debug for OutputAsync {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("OutputAsync")
            .field("inner", &"..")
            .finish()
    }
}

impl Future for OutputAsync {
    type Output = io::Result<Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx)
    }
}

/// The standard input stream for spawned children.
///
/// This type implements the `Write` trait to pass data to the stdin handle of
/// a child process. Note that this type is also "futures aware" meaning that it
/// is both (a) nonblocking and (b) will panic if used off of a future's task.
#[derive(Debug)]
pub struct ChildStdin {
    inner: imp::ChildStdin,
}

/// The standard output stream for spawned children.
///
/// This type implements the `Read` trait to read data from the stdout handle
/// of a child process. Note that this type is also "futures aware" meaning
/// that it is both (a) nonblocking and (b) will panic if used off of a
/// future's task.
#[derive(Debug)]
pub struct ChildStdout {
    inner: imp::ChildStdout,
}

/// The standard error stream for spawned children.
///
/// This type implements the `Read` trait to read data from the stderr handle
/// of a child process. Note that this type is also "futures aware" meaning
/// that it is both (a) nonblocking and (b) will panic if used off of a
/// future's task.
#[derive(Debug)]
pub struct ChildStderr {
    inner: imp::ChildStderr,
}

impl Write for ChildStdin {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.get_mut().write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.get_mut().flush()
    }
}

impl AsyncWrite for ChildStdin {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl Read for ChildStdout {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.get_mut().read(buf)
    }
}

impl AsyncRead for ChildStdout {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl Read for ChildStderr {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.get_mut().read(buf)
    }
}

impl AsyncRead for ChildStderr {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[cfg(unix)]
mod sys {
    use super::{ChildStderr, ChildStdin, ChildStdout};
    use std::os::unix::io::{AsRawFd, RawFd};

    impl AsRawFd for ChildStdin {
        fn as_raw_fd(&self) -> RawFd {
            self.inner.get_ref().as_raw_fd()
        }
    }

    impl AsRawFd for ChildStdout {
        fn as_raw_fd(&self) -> RawFd {
            self.inner.get_ref().as_raw_fd()
        }
    }

    impl AsRawFd for ChildStderr {
        fn as_raw_fd(&self) -> RawFd {
            self.inner.get_ref().as_raw_fd()
        }
    }
}

#[cfg(windows)]
mod sys {
    use super::{ChildStderr, ChildStdin, ChildStdout};
    use std::os::windows::io::{AsRawHandle, RawHandle};

    impl AsRawHandle for ChildStdin {
        fn as_raw_handle(&self) -> RawHandle {
            self.inner.get_ref().as_raw_handle()
        }
    }

    impl AsRawHandle for ChildStdout {
        fn as_raw_handle(&self) -> RawHandle {
            self.inner.get_ref().as_raw_handle()
        }
    }

    impl AsRawHandle for ChildStderr {
        fn as_raw_handle(&self) -> RawHandle {
            self.inner.get_ref().as_raw_handle()
        }
    }
}

#[cfg(test)]
mod test {
    use super::ChildDropGuard;
    use crate::kill::Kill;
    use futures_util::future::FutureExt;
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;

    struct Mock {
        num_kills: usize,
        num_polls: usize,
        poll_result: Poll<Result<(), ()>>,
    }

    impl Mock {
        fn new() -> Self {
            Self::with_result(Poll::Pending)
        }

        fn with_result(result: Poll<Result<(), ()>>) -> Self {
            Self {
                num_kills: 0,
                num_polls: 0,
                poll_result: result,
            }
        }
    }

    impl Kill for Mock {
        fn kill(&mut self) -> io::Result<()> {
            self.num_kills += 1;
            Ok(())
        }
    }

    impl Future for Mock {
        type Output = Result<(), ()>;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            let inner = Pin::get_mut(self);
            inner.num_polls += 1;
            inner.poll_result
        }
    }

    #[test]
    fn kills_on_drop() {
        let mut mock = Mock::new();

        {
            let guard = ChildDropGuard::new(&mut mock);
            drop(guard);
        }

        assert_eq!(1, mock.num_kills);
        assert_eq!(0, mock.num_polls);
    }

    #[test]
    fn no_kill_if_already_killed() {
        let mut mock = Mock::new();

        {
            let mut guard = ChildDropGuard::new(&mut mock);
            let _ = guard.kill();
            drop(guard);
        }

        assert_eq!(1, mock.num_kills);
        assert_eq!(0, mock.num_polls);
    }

    #[test]
    fn no_kill_if_reaped() {
        let mut mock_pending = Mock::with_result(Poll::Pending);
        let mut mock_reaped = Mock::with_result(Poll::Ready(Ok(())));
        let mut mock_err = Mock::with_result(Poll::Ready(Err(())));

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        {
            let mut guard = ChildDropGuard::new(&mut mock_pending);
            let _ = guard.poll_unpin(&mut context);

            let mut guard = ChildDropGuard::new(&mut mock_reaped);
            let _ = guard.poll_unpin(&mut context);

            let mut guard = ChildDropGuard::new(&mut mock_err);
            let _ = guard.poll_unpin(&mut context);
        }

        assert_eq!(1, mock_pending.num_kills);
        assert_eq!(1, mock_pending.num_polls);

        assert_eq!(0, mock_reaped.num_kills);
        assert_eq!(1, mock_reaped.num_polls);

        assert_eq!(1, mock_err.num_kills);
        assert_eq!(1, mock_err.num_polls);
    }

    #[test]
    fn no_kill_on_forget() {
        let mut mock = Mock::new();

        {
            let mut guard = ChildDropGuard::new(&mut mock);
            guard.forget();
            drop(guard);
        }

        assert_eq!(0, mock.num_kills);
        assert_eq!(0, mock.num_polls);
    }
}
