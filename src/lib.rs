//! An implementation of process management for Tokio.
//!
//! This crate provides `Future` implementations for spawning and waiting
//! on child processes. These implementations are powered by system APIs on
//! Windows and by signals on Unix systems.
//!
//! # Usage
//!
//! To achieve efficient polling of running child processes, we will need to
//! set up an event loop from `tokio-core`:
// FIXME: add warning that on Unix systems the *first* event loop can't go away?
//!
//! ```no_run
//! extern crate futures;
//! extern crate tokio_core;
//! extern crate tokio_process;
//!
//! use futures::Future;
//! use tokio_core::reactor::Core;
//! use tokio_process::Command;
//!
//! fn main() {
//!     let mut event_loop = Core::new().expect("failed to init event loop!");
//!     let mut cmd = Command::new("echo", &event_loop.handle());
//!     cmd.args(&["hello", "world"]);
//!
//!     match event_loop.run(cmd.spawn().flatten()) {
//!         Ok(status) => println!("exited successfully: {}", status.success()),
//!         Err(e) => panic!("failed to run command: {}", e),
//!     }
//! }
//! ```

#[macro_use]
extern crate futures;
extern crate tokio_core;
extern crate mio;
#[macro_use]
extern crate log;

use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{self, ExitStatus};

use futures::{Future, Poll};
use tokio_core::reactor::Handle;

#[path = "unix.rs"]
#[cfg(unix)]
mod imp;

#[path = "windows.rs"]
#[cfg(windows)]
mod imp;

pub struct Command {
    inner: process::Command,
    #[allow(dead_code)]
    handle: Handle,
}

/// A future that represents a spawned child process.
///
/// This future is created by the `Command::spawn` method.
///
/// If the caller does not care about the intermediate handle to a spawned
/// child, this future can be `flatten`ed to directly compute the child's
/// exit status.
pub struct Spawn {
    inner: Box<Future<Item=Child, Error=io::Error>>,
}

/// A future that represents the exit status of a running or exited child process.
///
/// This future is created by successfully polling the `Spawn` future.
///
/// # Note
///
/// Take note that there is no implementation of `Drop` for this future,
/// so if you do not ensure the `Child` has exited then it will continue to
/// run, even after the `Child` handle to the child process has gone out of
/// scope.
pub struct Child {
    inner: imp::Child,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
}

pub struct ChildStdin {
    inner: imp::ChildStdin,
}

pub struct ChildStdout {
    inner: imp::ChildStdout,
}

pub struct ChildStderr {
    inner: imp::ChildStderr,
}

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

    pub fn stdin(&mut self, cfg: process::Stdio) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }

    pub fn stdout(&mut self, cfg: process::Stdio) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }
    pub fn stderr(&mut self, cfg: process::Stdio) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }

    pub fn spawn(self) -> Spawn {
        Spawn {
            inner: Box::new(imp::spawn(self)),
        }
    }
}

impl Future for Spawn {
    type Item = Child;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Child, io::Error> {
        self.inner.poll()
    }
}

impl Child {
    /// Returns the OS-assigned process identifier associated with this child.
    pub fn id(&self) -> u32 {
        self.inner.id()
    }

    /// Forces the child to exit. This is equivalent to sending a
    /// SIGKILL on unix platforms.
    pub fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }

    pub fn stdin(&mut self) -> &mut Option<ChildStdin> {
        &mut self.stdin
    }

    pub fn stdout(&mut self) -> &mut Option<ChildStdout> {
        &mut self.stdout
    }

    pub fn stderr(&mut self) -> &mut Option<ChildStderr> {
        &mut self.stderr
    }
}

impl Future for Child {
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<ExitStatus, io::Error> {
        self.inner.poll()
    }
}

impl Write for ChildStdin {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Read for ChildStdout {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}

impl Read for ChildStderr {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.inner.read(bytes)
    }
}
