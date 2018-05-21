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
use mio::unix::{EventedFd, UnixReady};
use mio::{PollOpt, Ready, Token};
use mio::event::Evented;
use mio;
use self::tokio_signal::unix::Signal;
use std::fmt;
use tokio_io::IoFuture;
use tokio_reactor::{Handle, PollEvented};

#[must_use = "futures do nothing unless polled"]
pub struct Child {
    inner: process::Child,
    reaped: bool,
    sigchld: FlattenStream<IoFuture<Signal>>,
}

impl fmt::Debug for Child {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Child")
            .field("pid", &self.inner.id())
            .field("inner", &self.inner)
            .field("reaped", &self.reaped)
            .field("sigchld", &"..")
            .finish()
    }
}

impl Child {
    pub fn new(inner: process::Child, handle: &Handle) -> Child {
        Child {
            inner: inner,
            reaped: false,
            sigchld: Signal::with_handle(libc::SIGCHLD, handle).flatten_stream(),
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
        if !self.reaped {
            // NB: SIGKILL cannnot be caught, so the process will definitely exit immediately.
            // We're not waiting for the process itself but for the kernel to execute the kill.
            self.inner.kill()?;
            let _ = self.try_wait(true);
        }

        Ok(())
    }

    pub fn poll_exit(&mut self) -> Poll<ExitStatus, io::Error> {
        loop {
            // Ensure we don't register for additional notifications
            // if the child has already finished.
            if self.reaped {
                return Ok(Async::NotReady);
            }

            // If the child hasn't exited yet, then it's our responsibility to
            // ensure the current task gets notified when it might be able to
            // make progress.
            //
            // As described in `spawn` above, we just indicate that we can
            // next make progress once a SIGCHLD is received.
            //
            // However, we will register for a notification on the next signal
            // BEFORE we poll the child. Otherwise it is possible that the child
            // can exit and the signal can arrive after we last polled the child,
            // but before we've registered for a notification on the next signal
            // (this can cause a deadlock if there are no more spawned children
            // which can generate a different signal for us). A side effect of
            // pre-registering for signal notifications is that when the child
            // exits, we will have already registered for an additional
            // notification we don't need to consume. If another signal arrives,
            // this future's task will be notified/woken up again. Since the
            // futures model allows for spurious wake ups this extra wakeup
            // should not cause significant issues with parent futures.
            let registered_interest = try!(self.sigchld.poll()).is_not_ready();

            if let Some(e) = try!(self.try_wait(false)) {
                return Ok(e.into());
            }

            // If our attempt to poll for the next signal was not ready, then
            // we've arranged for our task to get notified and we can bail out.
            if registered_interest {
                return Ok(Async::NotReady);
            } else {
                // Otherwise, if the signal stream delivered a signal to us, we
                // won't get notified at the next signal, so we'll loop and try
                // again.
                continue;
            }
        }
    }

    fn try_wait(&mut self, block_on_wait: bool) -> io::Result<Option<ExitStatus>> {
        assert!(!self.reaped);
        let exit = try!(try_wait_process(self.id() as libc::pid_t, block_on_wait));

        if let Some(_) = exit {
            self.reaped = true;
        }

        Ok(exit)
    }
}

fn try_wait_process(id: libc::pid_t, block_on_wait: bool) -> io::Result<Option<ExitStatus>> {
    let wait_flags = if block_on_wait { 0 } else { libc::WNOHANG };
    let mut status = 0;

    loop {
        match unsafe { libc::waitpid(id, &mut status, wait_flags) } {
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
                                                interest | UnixReady::hup(),
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
                                                  interest | UnixReady::hup(),
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
    let io = try!(PollEvented::new_with_handle(Fd(io), handle));
    Ok(Some(io))
}
