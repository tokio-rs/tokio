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

use std::io;
use std::os::unix::prelude::*;
use std::process::{self, ExitStatus};

use futures::future::FlattenStream;
use futures::{Future, Poll, Async, Stream};
use mio::unix::EventedFd;
use mio::{Evented, PollOpt, Ready, Token};
use mio;
use self::libc::c_int;
use self::tokio_signal::unix::Signal;
use tokio_core::io::IoFuture;
use tokio_core::reactor::{Handle, PollEvented};

pub struct Child {
    inner: process::Child,
    reaped: bool,
    sigchld: FlattenStream<IoFuture<Signal>>,
}

impl Child {
    pub fn new(inner: process::Child, handle: &Handle) -> Child {
        Child {
            inner: inner,
            reaped: false,
            sigchld: Signal::new(libc::SIGCHLD, handle).flatten_stream(),
        }
    }

    pub fn register_stdin(&mut self, handle: &Handle)
                          -> io::Result<Option<ChildStdin>> {
        stdio(self.inner.stdin.take(), handle)
    }

    pub fn register_stdout(&mut self, handle: &Handle)
                           -> io::Result<Option<ChildStdout>> {
        stdio(self.inner.stdout.take(), handle)
    }

    pub fn register_stderr(&mut self, handle: &Handle)
                           -> io::Result<Option<ChildStderr>> {
        stdio(self.inner.stderr.take(), handle)
    }

    pub fn id(&self) -> u32 {
        self.inner.id()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        if self.reaped {
            Ok(())
        } else {
            self.inner.kill()
        }
    }

    pub fn poll_exit(&mut self) -> Poll<ExitStatus, io::Error> {
        assert!(!self.reaped);
        loop {
            // Ensure that once we've successfully waited we won't try to
            // `kill` above.
            if let Some(e) = try!(self.try_wait()) {
                self.reaped = true;
                return Ok(e.into())
            }

            // If the child hasn't exited yet, then it's our responsibility to
            // ensure the current task gets notified when it might be able to
            // make progress.
            //
            // As described in `spawn` above, we just indicate that we can
            // next make progress once a SIGCHLD is received.
            if try!(self.sigchld.poll()).is_not_ready() {
                return Ok(Async::NotReady)
            }
        }
    }

    fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        let id = self.id() as c_int;
        let mut status = 0;
        loop {
            match unsafe { libc::waitpid(id, &mut status, libc::WNOHANG) } {
                0 => return Ok(None),
                n if n < 0 => {
                    let err = io::Error::last_os_error();
                    if err.kind() == io::ErrorKind::Interrupted {
                        continue
                    }
                    return Err(err)
                }
                n => {
                    assert_eq!(n, id);
                    return Ok(Some(ExitStatus::from_raw(status)))
                }
            }
        }
    }
}

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
        EventedFd(&self.0.as_raw_fd()).register(poll,
                                                token,
                                                interest | Ready::hup(),
                                                opts)
    }

    fn reregister(&self,
                  poll: &mio::Poll,
                  token: Token,
                  interest: Ready,
                  opts: PollOpt)
                  -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).reregister(poll,
                                                  token,
                                                  interest | Ready::hup(),
                                                  opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).deregister(poll)
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
    let io = try!(PollEvented::new(Fd(io), handle));
    Ok(Some(io))
}
