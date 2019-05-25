//! Unix handling of child processes
//!
//! Right now the only "fancy" thing about this is how we implement the
//! `Future` implementation on `Child` to get the exit status. Unix offers
//! no way to register a child with epoll, and the only real way to get a
//! notification when a process exits is the SIGCHLD signal.
//!
//! Signal handling in general is *super* hairy and complicated, and it's even
//! more complicated here with the fact that signals are coalesced, so we may
//! not get a SIGCHLD-per-child.
//!
//! Our best approximation here is to check *all spawned processes* for all
//! SIGCHLD signals received. To do that we create a `Signal`, implemented in
//! the `tokio-signal` crate, which is a stream over signals being received.
//!
//! Later when we poll the process's exit status we simply check to see if a
//! SIGCHLD has happened since we last checked, and while that returns "yes" we
//! keep trying.
//!
//! Note that this means that this isn't really scalable, but then again
//! processes in general aren't scalable (e.g. millions) so it shouldn't be that
//! bad in theory...

extern crate libc;
extern crate tokio_signal;

mod reap;

use futures::future::FlattenStream;
use futures::{Future, Poll};
use mio::unix::{EventedFd, UnixReady};
use mio::{PollOpt, Ready, Token};
use mio::event::Evented;
use mio;
use self::reap::{EventedReaper, Kill, Wait};
use self::tokio_signal::unix::Signal;
use std::fmt;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::process::{self, ExitStatus};
use tokio_io::IoFuture;
use tokio_reactor::{Handle, PollEvented};

impl Wait for process::Child {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.try_wait()
    }
}

impl Kill for process::Child {
    fn kill(&mut self) -> io::Result<()> {
        self.kill()
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct Child {
    inner: EventedReaper<process::Child, FlattenStream<IoFuture<Signal>>>,
}

impl fmt::Debug for Child {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Child")
            .field("pid", &self.inner.id())
            .finish()
    }
}

impl Child {
    pub fn new(mut inner: process::Child, handle: &Handle)
        -> io::Result<(Child, Option<ChildStdin>, Option<ChildStdout>, Option<ChildStderr>)>
    {
        let stdin = stdio(inner.stdin.take(), handle)?;
        let stdout = stdio(inner.stdout.take(), handle)?;
        let stderr = stdio(inner.stderr.take(), handle)?;

        let signal = Signal::with_handle(libc::SIGCHLD, handle).flatten_stream();
        let child = Child {
            inner: EventedReaper::new(inner, signal),
        };

        Ok((child, stdin, stdout, stderr))
    }

    pub fn id(&self) -> u32 {
        self.inner.id()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }

    pub fn poll_exit(&mut self) -> Poll<ExitStatus, io::Error> {
        self.inner.poll()
    }
}

#[derive(Debug)]
pub struct Fd<T>(T);

impl<T: io::Read> io::Read for Fd<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}

impl<T: io::Write> io::Write for Fd<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<T> AsRawFd for Fd<T> where T: AsRawFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

pub type ChildStdin = PollEvented<Fd<process::ChildStdin>>;
pub type ChildStdout = PollEvented<Fd<process::ChildStdout>>;
pub type ChildStderr = PollEvented<Fd<process::ChildStderr>>;

impl<T> Evented for Fd<T> where T: AsRawFd {
    fn register(&self,
                poll: &mio::Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt)
                -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll,
                                              token,
                                              interest | UnixReady::hup(),
                                              opts)
    }

    fn reregister(&self,
                  poll: &mio::Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt)
                  -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll,
                                                token,
                                                interest | UnixReady::hup(),
                                                opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}

fn stdio<T>(option: Option<T>, handle: &Handle)
            -> io::Result<Option<PollEvented<Fd<T>>>>
    where T: AsRawFd
{
    let io = match option {
        Some(io) => io,
        None => return Ok(None),
    };

    // Set the fd to nonblocking before we pass it to the event loop
    unsafe {
        let fd = io.as_raw_fd();
        let r = libc::fcntl(fd, libc::F_GETFL);
        if r == -1 {
            return Err(io::Error::last_os_error())
        }
        let r = libc::fcntl(fd, libc::F_SETFL, r | libc::O_NONBLOCK);
        if r == -1 {
            return Err(io::Error::last_os_error())
        }
    }
    let io = try!(PollEvented::new_with_handle(Fd(io), handle));
    Ok(Some(io))
}
