#![cfg(unix)]

extern crate env_logger;
extern crate futures;
extern crate nix;
extern crate mio;
extern crate tokio_core;

use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::time::Duration;
use std::thread;

use mio::{Evented, PollOpt, Ready, Token};
use mio::unix::EventedFd;
use nix::fcntl::{fcntl, O_NONBLOCK};
use nix::fcntl::FcntlArg::F_SETFL;
use nix::unistd::{close, pipe, read, write};

use tokio_core::io::read_to_end;
use tokio_core::reactor::{Core, PollEvented};

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

fn set_nonblock(s: &AsRawFd) -> io::Result<()> {
    fcntl(s.as_raw_fd(), F_SETFL(O_NONBLOCK)).map_err(from_nix_error)
                                             .map(|_| ())
}

fn from_nix_error(err: nix::Error) -> io::Error {
    io::Error::from_raw_os_error(err.errno() as i32)
}

struct PipeSource(RawFd);

impl io::Read for PipeSource {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        read(self.as_raw_fd(), bytes).map_err(from_nix_error)
    }
}

pub struct StdStream<T> {
    io: PollEvented<RawFdWrap<T>>,
}

impl<T> io::Read for StdStream<T> where T: io::Read {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.read(buf)
    }
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

impl<T> Evented for RawFdWrap<T> where T: AsRawFd {
    fn register(&self, poll: &mio::Poll, token: Token, interest: Ready, opts: PollOpt)
                -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).register(poll, token, interest | Ready::hup(), opts)
    }
    fn reregister(&self, poll: &mio::Poll, token: Token, interest: Ready, opts: PollOpt)
                  -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).reregister(poll, token, interest | Ready::hup(), opts)
    }
    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).deregister(poll)
    }
}

impl AsRawFd for PipeSource {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

#[test]
fn hup() {
    drop(env_logger::init());

    let mut l = t!(Core::new());
    let (source, sink) = pipe().unwrap();
    let t = thread::spawn(move || {
        write(sink, b"Hello!\n").unwrap();
        write(sink, b"Good bye!\n").unwrap();
        thread::sleep(Duration::from_millis(100));
        close(sink).unwrap();
    });

    let source = StdStream {
        io: PollEvented::new(RawFdWrap::new(PipeSource(source)).unwrap(), &l.handle()).unwrap()
    };

    let reader = read_to_end(source, Vec::new());
    let (_, content) = t!(l.run(reader));
    assert_eq!(&b"Hello!\nGood bye!\n"[..], &content[..]);
    t.join().unwrap();
}
