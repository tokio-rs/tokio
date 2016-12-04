#[macro_use]
extern crate futures;
extern crate tokio_core;
extern crate mio;
#[macro_use]
extern crate log;

use std::ffi::OsStr;
use std::io;
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

pub use imp::ChildStdin;
pub use imp::ChildStdout;

pub struct Command {
    inner: process::Command,
    #[allow(dead_code)]
    handle: Handle,
}

pub struct Spawn {
    inner: Box<Future<Item=Child, Error=io::Error>>,
}

pub struct Child {
    inner: imp::Child,
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
            inner: Box::new(imp::spawn(self).map(|c| Child { inner: c })),
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
    pub fn id(&self) -> u32 {
        self.inner.id()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }

    pub fn stdin(&mut self) -> &mut Option<imp::ChildStdin> {
        &mut self.inner.stdin
    }

    pub fn stdout(&mut self) -> &mut Option<imp::ChildStdout> {
        &mut self.inner.stdout
    }

    pub fn stderr(&mut self) -> &mut Option<imp::ChildStderr> {
        &mut self.inner.stderr
    }
}

impl Future for Child {
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<ExitStatus, io::Error> {
        self.inner.poll()
    }
}
