extern crate libc;
extern crate nix;
extern crate tokio_signal;

use std::io;
use std::os::unix::prelude::*;
use std::process::{self, ExitStatus};

use futures::stream::Stream;
use futures::{Future, Poll, Async};
use tokio_core::reactor::{Handle,PollEvented};
use self::libc::c_int;
use self::nix::fcntl::FcntlArg::F_SETFL;
use self::nix::fcntl::{fcntl, O_NONBLOCK};
use self::tokio_signal::unix::Signal;

use mio;
use mio::{Evented, PollOpt, Ready, Token};
use mio::unix::EventedFd;

use Command;

pub struct Child {
    child: process::Child,
    reaped: bool,
    sigchld: Signal,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

struct RawFdWrap<T>(T);

impl<T> RawFdWrap<T> {
    fn new(fd: T) -> io::Result<Self>
        where T: AsRawFd {

        try!(set_nonblock(&fd));
        Ok(RawFdWrap(fd))
    }
}

impl<T> io::Read for RawFdWrap<T> where T: io::Read {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}

impl<T> io::Write for RawFdWrap<T> where T: io::Write {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

fn from_nix_error(err: nix::Error) -> io::Error {
    io::Error::from_raw_os_error(err.errno() as i32)
}

fn set_nonblock(s: &AsRawFd) -> io::Result<()> {
    fcntl(s.as_raw_fd(), F_SETFL(O_NONBLOCK)).map_err(from_nix_error)
                                             .map(|_| ())
}

pub struct StdStream<T> {
    io: PollEvented<RawFdWrap<T>>,
}

pub type ChildStdin = StdStream<process::ChildStdin>;
pub type ChildStdout = StdStream<process::ChildStdout>;
pub type ChildStderr = StdStream<process::ChildStderr>;

impl<T> Evented for RawFdWrap<T> where T: AsRawFd {
    fn register(&self, poll: &mio::Poll, token: Token, interest: Ready, opts: PollOpt)
                -> io::Result<()> {
        debug!("Evented::register({:?}, {:?}, {:?}", token, interest, opts);
        EventedFd(&self.0.as_raw_fd()).register(poll, token, interest | Ready::hup(), opts)
    }
    fn reregister(&self, poll: &mio::Poll, token: Token, interest: Ready, opts: PollOpt)
                  -> io::Result<()> {
        debug!("Evented::reregister({:?}, {:?}, {:?}", token, interest, opts);
        EventedFd(&self.0.as_raw_fd()).reregister(poll, token, interest | Ready::hup(), opts)
    }
    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        debug!("Evented::deregister()");
        EventedFd(&self.0.as_raw_fd()).deregister(poll)
    }
}

impl<T> io::Read for StdStream<T> where T: io::Read {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.read(buf)
    }
}

impl<T> io::Write for StdStream<T> where T: io::Write {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}

fn stdio<T>(option: &mut Option<T>, handle: &Handle) -> Result<Option<StdStream<T>>, io::Error>
    where T: AsRawFd {

    option.take().map_or(Ok(None), |stream| {
        PollEvented::new(try!(RawFdWrap::new(stream)), handle).map(|io| {
            Some(StdStream { io: io })
        })
    })
}

/// Spawns a new child process.
///
/// Right now the only "fancy" thing about this is how we implement the
/// `Future` implementation on `Child` to get the exit status. Unix offers
/// no way to register a child with epoll, and the only real way to get a
/// notification when a process exits is the SIGCHLD signal.
///
/// Signal handling in general is *super* hairy and complicated, and it's even
/// more complicated here with the fact that signals are coalesced, so we may
/// not get a SIGCHLD-per-child.
///
/// Our best approximation here is to check *all spawned processes* for all
/// SIGCHLD signals received. To do that we create a `Signal`, implemented in
/// the `tokio-signal` crate, which is a stream over signals being received.
///
/// Later when we poll the process's exit status we simply check to see if a
/// SIGCHLD has happened since we last checked, and while that returns "yes" we
/// keep trying.
///
/// Note that this means that this isn't really scalable, but then again
/// processes in general aren't scalable (e.g. millions) so it shouldn't be that
/// bad in theory...
pub fn spawn(mut cmd: Command) -> Box<Future<Item=Child, Error=io::Error>> {
    Box::new(Signal::new(libc::SIGCHLD, &cmd.handle).and_then(move |sigchld| {
        cmd.inner.spawn().and_then(|mut c| {
            let stdin = try!(stdio(&mut c.stdin, &cmd.handle));
            let stdout = try!(stdio(&mut c.stdout, &cmd.handle));
            let stderr = try!(stdio(&mut c.stderr, &cmd.handle));
            Ok(Child {
                child: c,
                reaped: false,
                sigchld: sigchld,
                stdin: stdin,
                stdout: stdout,
                stderr: stderr,
            })
        })
    }))
}

impl Child {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        if self.reaped {
            Ok(())
        } else {
            self.child.kill()
        }
    }
}

impl Future for Child {
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<ExitStatus, io::Error> {
        assert!(!self.reaped);
        loop {
            // Ensure that once we've successfully waited we won't try to
            // `kill` above.
            if let Some(e) = try!(try_wait(&self.child)) {
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
}

pub fn try_wait(child: &process::Child) -> io::Result<Option<ExitStatus>> {
    let id = child.id() as c_int;
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
