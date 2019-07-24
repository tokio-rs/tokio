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
extern crate mio;
extern crate tokio_signal;

mod orphan;
mod reap;

use self::mio::event::Evented;
use self::mio::unix::{EventedFd, UnixReady};
use self::mio::{Poll as MioPoll, PollOpt, Ready, Token};
use self::orphan::{AtomicOrphanQueue, OrphanQueue, Wait};
use self::reap::Reaper;
use self::tokio_signal::unix::Signal;
use super::SpawnedChild;
use crate::kill::Kill;
use futures::future::FlattenStream;
use futures::{Future, Poll};
use std::fmt;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::process::{self, ExitStatus};
use tokio_io::IoFuture;
use tokio_reactor::{Handle, PollEvented};

impl Wait for process::Child {
    fn id(&self) -> u32 {
        self.id()
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.try_wait()
    }
}

impl Kill for process::Child {
    fn kill(&mut self) -> io::Result<()> {
        self.kill()
    }
}

lazy_static! {
    static ref ORPHAN_QUEUE: AtomicOrphanQueue<process::Child> = AtomicOrphanQueue::new();
}

struct GlobalOrphanQueue;

impl fmt::Debug for GlobalOrphanQueue {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        ORPHAN_QUEUE.fmt(fmt)
    }
}

impl OrphanQueue<process::Child> for GlobalOrphanQueue {
    fn push_orphan(&self, orphan: process::Child) {
        ORPHAN_QUEUE.push_orphan(orphan)
    }

    fn reap_orphans(&self) {
        ORPHAN_QUEUE.reap_orphans()
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct Child {
    inner: Reaper<process::Child, GlobalOrphanQueue, FlattenStream<IoFuture<Signal>>>,
}

impl fmt::Debug for Child {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Child")
            .field("pid", &self.inner.id())
            .finish()
    }
}

pub(crate) fn spawn_child(cmd: &mut process::Command, handle: &Handle) -> io::Result<SpawnedChild> {
    let mut child = cmd.spawn()?;
    let stdin = stdio(child.stdin.take(), handle)?;
    let stdout = stdio(child.stdout.take(), handle)?;
    let stderr = stdio(child.stderr.take(), handle)?;

    let signal = Signal::with_handle(libc::SIGCHLD, handle).flatten_stream();
    Ok(SpawnedChild {
        child: Child {
            inner: Reaper::new(child, GlobalOrphanQueue, signal),
        },
        stdin,
        stdout,
        stderr,
    })
}

impl Child {
    pub fn id(&self) -> u32 {
        self.inner.id()
    }
}

impl Kill for Child {
    fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }
}

impl Future for Child {
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
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

impl<T> AsRawFd for Fd<T>
where
    T: AsRawFd,
{
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

pub type ChildStdin = PollEvented<Fd<process::ChildStdin>>;
pub type ChildStdout = PollEvented<Fd<process::ChildStdout>>;
pub type ChildStderr = PollEvented<Fd<process::ChildStderr>>;

impl<T> Evented for Fd<T>
where
    T: AsRawFd,
{
    fn register(
        &self,
        poll: &MioPoll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll, token, interest | UnixReady::hup(), opts)
    }

    fn reregister(
        &self,
        poll: &MioPoll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest | UnixReady::hup(), opts)
    }

    fn deregister(&self, poll: &MioPoll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}

fn stdio<T>(option: Option<T>, handle: &Handle) -> io::Result<Option<PollEvented<Fd<T>>>>
where
    T: AsRawFd,
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
            return Err(io::Error::last_os_error());
        }
        let r = libc::fcntl(fd, libc::F_SETFL, r | libc::O_NONBLOCK);
        if r == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    let io = PollEvented::new_with_handle(Fd(io), handle)?;
    Ok(Some(io))
}
