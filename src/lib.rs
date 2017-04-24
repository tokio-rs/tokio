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
//! for it using an event loop.
//!
//! ```no_run
//! extern crate futures;
//! extern crate tokio_core;
//! extern crate tokio_process;
//!
//! use std::process::Command;
//!
//! use futures::Future;
//! use tokio_core::reactor::Core;
//! use tokio_process::CommandExt;
//!
//! fn main() {
//!     // Create our own local event loop
//!     let mut core = Core::new().unwrap();
//!
//!     // Use the standard library's `Command` type to build a process and
//!     // then execute it via the `CommandExt` trait.
//!     let child = Command::new("echo").arg("hello").arg("world")
//!                         .spawn_async(&core.handle());
//!
//!     // Make sure our child succeeded in spawning
//!     let child = child.expect("failed to spawn");
//!
//!     match core.run(child) {
//!         Ok(status) => println!("exit status: {}", status),
//!         Err(e) => panic!("failed to wait for exit: {}", e),
//!     }
//! }
//! ```
//!
//! Next, let's take a look at an example where we not only spawn `echo hello
//! world` but we also capture its output.
//!
//! ```no_run
//! extern crate futures;
//! extern crate tokio_core;
//! extern crate tokio_process;
//!
//! use std::process::Command;
//!
//! use futures::Future;
//! use tokio_core::reactor::Core;
//! use tokio_process::CommandExt;
//!
//! fn main() {
//!     let mut core = Core::new().unwrap();
//!
//!     // Like above, but use `output_async` which returns a future instead of
//!     // immediately returning the `Child`.
//!     let output = Command::new("echo").arg("hello").arg("world")
//!                         .output_async(&core.handle());
//!     let output = core.run(output).expect("failed to collect output");
//!
//!     assert!(output.status.success());
//!     assert_eq!(output.stdout, b"hello world\n");
//! }
//! ```
//!
//! We can also read input line by line.
//!
//! ```no_run
//! extern crate futures;
//! extern crate tokio_core;
//! extern crate tokio_process;
//! extern crate tokio_io;
//!
//! use std::io;
//! use std::process::{Command, Stdio, Output};
//!
//! use futures::{BoxFuture, Future, Stream};
//! use tokio_core::reactor::Core;
//! use tokio_process::{CommandExt, Child};
//!
//! fn get_lines(mut cat: Child) -> BoxFuture<((), Output), io::Error> {
//!     let stdout = cat.stdout().take().unwrap();
//!     let reader = io::BufReader::new(stdout);
//!     let lines = tokio_io::io::lines(reader);
//!     let cycle = lines.for_each(|l| {
//!         println!("Line: {}", l);
//!         Ok(())
//!     });
//!     cycle.join(cat.wait_with_output()).boxed()
//! }
//!
//! fn main() {
//!     let mut cmd = Command::new("cat");
//!     let mut cat = cmd.stdout(Stdio::piped());
//!     let mut core = Core::new().unwrap();
//!     let child = cat.spawn_async(&core.handle()).unwrap();
//!     core.run(get_lines(child)).unwrap();
//! }
//!
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

#![deny(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tokio-process/0.1")]

#[macro_use]
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate mio;
#[macro_use]
extern crate log;

use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{self, ExitStatus, Output, Stdio};

use futures::{Future, Poll, IntoFuture};
use futures::future::{Flatten, FutureResult, Either, ok};
use tokio_core::reactor::Handle;
use tokio_io::io::{read_to_end};
use tokio_io::{AsyncWrite, AsyncRead, IoFuture};

#[path = "unix.rs"]
#[cfg(unix)]
mod imp;

#[path = "windows.rs"]
#[cfg(windows)]
mod imp;

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
    /// The `handle` specified to this method must be a handle to a valid event
    /// loop, and all I/O this child does will be associated with the specified
    /// event loop.
    fn spawn_async(&mut self, handle: &Handle) -> io::Result<Child>;

    /// Executes a command as a child process, waiting for it to finish and
    /// collecting its exit status.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    ///
    /// The `StatusAsync` future returned will resolve to the `ExitStatus`
    /// type in the standard library representing how the process exited. If
    /// output handles are set to a pipe then they will be immediately closed
    /// after the child is spawned.
    ///
    /// The `handle` specified must be a handle to a valid event loop, and all
    /// I/O this child does will be associated with the specified event loop.
    ///
    /// If the `OutputAsync` future is dropped before the future resolves, then
    /// the child will be killed, if it was spawned.
    fn status_async(&mut self, handle: &Handle) -> StatusAsync;

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
    fn output_async(&mut self, handle: &Handle) -> OutputAsync;
}


impl CommandExt for process::Command {
    fn spawn_async(&mut self, handle: &Handle) -> io::Result<Child> {
        let mut child = Child {
            child: imp::Child::new(try!(self.spawn()), handle),
            stdin: None,
            stdout: None,
            stderr: None,
            kill_on_drop: true,
        };
        child.stdin = try!(child.child.register_stdin(handle)).map(|io| {
            ChildStdin { inner: io }
        });
        child.stdout = try!(child.child.register_stdout(handle)).map(|io| {
            ChildStdout { inner: io }
        });
        child.stderr = try!(child.child.register_stderr(handle)).map(|io| {
            ChildStderr { inner: io }
        });
        Ok(child)
    }

    fn status_async(&mut self, handle: &Handle) -> StatusAsync {
        StatusAsync {
            inner: self.spawn_async(handle).into_future().flatten(),
        }
    }

    fn output_async(&mut self, handle: &Handle) -> OutputAsync {
        self.stdout(Stdio::piped());
        self.stderr(Stdio::piped());
        OutputAsync {
            inner: self.spawn_async(handle).into_future().and_then(|c| {
                c.wait_with_output()
            }).boxed(),
        }
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
pub struct Child {
    child: imp::Child,
    kill_on_drop: bool,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
}

impl Child {
    /// Returns the OS-assigned process identifier associated with this child.
    pub fn id(&self) -> u32 {
        self.child.id()
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
        let stdout = match self.stdout().take() {
            Some(io) => Either::A(read_to_end(io, Vec::new()).map(|p| p.1)),
            None => Either::B(ok(Vec::new())),
        };
        let stderr = match self.stderr().take() {
            Some(io) => Either::A(read_to_end(io, Vec::new()).map(|p| p.1)),
            None => Either::B(ok(Vec::new())),
        };

        WaitWithOutput {
            inner: self.join3(stdout, stderr).map(|(status, stdout, stderr)| {
                Output {
                    status: status,
                    stdout: stdout,
                    stderr: stderr,
                }
            }).boxed()
        }
    }

    /// Drop this `Child` without killing the underlying process.
    ///
    /// Normally a `Child` is killed if it's still alive when dropped, but this
    /// method will ensure that the child may continue running once the `Child`
    /// instance is dropped.
    pub fn forget(mut self) {
        self.kill_on_drop = false;
    }
}

impl Future for Child {
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<ExitStatus, io::Error> {
        self.child.poll_exit()
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        if self.kill_on_drop {
            drop(self.kill());
        }
    }
}

/// Future returned from the `Child::wait_with_output` method.
///
/// This future will resolve to the standard library's `Output` type which
/// contains the exit status, stdout, and stderr of a child process.
pub struct WaitWithOutput {
    inner: IoFuture<Output>,
}

impl Future for WaitWithOutput {
    type Item = Output;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Output, io::Error> {
        self.inner.poll()
    }
}

/// Future returned by the `CommandExt::status_async` method.
///
/// This future is used to conveniently spawn a child and simply wait for its
/// exit status. This future will resolves to the `ExitStatus` type in the
/// standard library.
pub struct StatusAsync {
    inner: Flatten<FutureResult<Child, io::Error>>,
}

impl Future for StatusAsync {
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<ExitStatus, io::Error> {
        self.inner.poll()
    }
}

/// Future returned by the `CommandExt::output_async` method.
///
/// This future is mostly equivalent to spawning a process and then calling
/// `wait_with_output` on it internally. This can be useful to simply spawn a
/// process, collecting all of its output and its exit status.
pub struct OutputAsync {
    inner: IoFuture<Output>,
}

impl Future for OutputAsync {
    type Item = Output;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Output, io::Error> {
        self.inner.poll()
    }
}

/// The standard input stream for spawned children.
///
/// This type implements the `Write` trait to pass data to the stdin handle of
/// a child process. Note that this type is also "futures aware" meaning that it
/// is both (a) nonblocking and (b) will panic if used off of a future's task.
pub struct ChildStdin {
    inner: imp::ChildStdin,
}

/// The standard output stream for spawned children.
///
/// This type implements the `Read` trait to read data from the stdout handle
/// of a child process. Note that this type is also "futures aware" meaning
/// that it is both (a) nonblocking and (b) will panic if used off of a
/// future's task.
pub struct ChildStdout {
    inner: imp::ChildStdout,
}

/// The standard error stream for spawned children.
///
/// This type implements the `Read` trait to read data from the stderr handle
/// of a child process. Note that this type is also "futures aware" meaning
/// that it is both (a) nonblocking and (b) will panic if used off of a
/// future's task.
pub struct ChildStderr {
    inner: imp::ChildStderr,
}

impl Write for ChildStdin {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl AsyncWrite for ChildStdin {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.inner.shutdown()
    }
}

impl Read for ChildStdout {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}

impl AsyncRead for ChildStdout {
}

impl Read for ChildStderr {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}

impl AsyncRead for ChildStderr {
}

// deprecated from 0.1.0

#[deprecated(note = "use std::process::Command instead")]
#[allow(deprecated, missing_docs)]
#[doc(hidden)]
pub struct Command {
    inner: process::Command,
    #[allow(dead_code)]
    handle: Handle,
}

#[deprecated(note = "use std::process::Command instead")]
#[allow(deprecated, missing_docs)]
#[doc(hidden)]
pub struct Spawn {
    inner: Box<Future<Item=Child, Error=io::Error>>,
}

#[deprecated(note = "use std::process::Command instead")]
#[allow(deprecated, missing_docs)]
#[doc(hidden)]
impl Command {
    pub fn new<T: AsRef<OsStr>>(exe: T, handle: &Handle) -> Command {
        Command::_new(exe.as_ref(), handle)
    }

    fn _new(exe: &OsStr, handle: &Handle) -> Command {
        Command {
            inner: process::Command::new(exe),
            handle: handle.clone(),
        }
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Command {
        self._arg(arg.as_ref())
    }

    fn _arg(&mut self, arg: &OsStr) -> &mut Command {
        self.inner.arg(arg);
        self
    }

    pub fn args<S: AsRef<OsStr>>(&mut self, args: &[S]) -> &mut Command {
        for arg in args {
            self._arg(arg.as_ref());
        }
        self
    }

    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Command
        where K: AsRef<OsStr>, V: AsRef<OsStr>
    {
        self._env(key.as_ref(), val.as_ref())
    }

    fn _env(&mut self, key: &OsStr, val: &OsStr) -> &mut Command {
        self.inner.env(key, val);
        self
    }

    pub fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Command {
        self._env_remove(key.as_ref())
    }

    fn _env_remove(&mut self, key: &OsStr) -> &mut Command {
        self.inner.env_remove(key);
        self
    }

    pub fn env_clear(&mut self) -> &mut Command {
        self.inner.env_clear();
        self
    }

    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Command {
        self._current_dir(dir.as_ref())
    }

    fn _current_dir(&mut self, dir: &Path) -> &mut Command {
        self.inner.current_dir(dir);
        self
    }

    pub fn spawn(mut self) -> Spawn {
        Spawn {
            inner: self.inner.spawn_async(&self.handle).into_future().boxed()
        }
    }
}

#[deprecated(note = "use std::process::Command instead")]
#[allow(deprecated)]
#[doc(hidden)]
impl Future for Spawn {
    type Item = Child;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Child, io::Error> {
        self.inner.poll()
    }
}

