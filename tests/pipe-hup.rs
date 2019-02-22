#![cfg(unix)]

extern crate env_logger;
extern crate futures;
extern crate libc;
extern crate mio;
extern crate tokio;
extern crate tokio_io;

use std::fs::File;
use std::io::{self, Write};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::thread;
use std::time::Duration;

use futures::Future;
use mio::event::Evented;
use mio::unix::{EventedFd, UnixReady};
use mio::{PollOpt, Ready, Token};
use tokio::reactor::{Handle, PollEvented2};
use tokio_io::io::read_to_end;

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
        }
    };
}

struct MyFile(File);

impl MyFile {
    fn new(file: File) -> MyFile {
        unsafe {
            let r = libc::fcntl(file.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK);
            assert!(r != -1, "fcntl error: {}", io::Error::last_os_error());
        }
        MyFile(file)
    }
}

impl io::Read for MyFile {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}

impl Evented for MyFile {
    fn register(
        &self,
        poll: &mio::Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        let hup: Ready = UnixReady::hup().into();
        EventedFd(&self.0.as_raw_fd()).register(poll, token, interest | hup, opts)
    }
    fn reregister(
        &self,
        poll: &mio::Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        let hup: Ready = UnixReady::hup().into();
        EventedFd(&self.0.as_raw_fd()).reregister(poll, token, interest | hup, opts)
    }
    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).deregister(poll)
    }
}

#[test]
fn hup() {
    drop(env_logger::try_init());

    let handle = Handle::default();
    unsafe {
        let mut pipes = [0; 2];
        assert!(
            libc::pipe(pipes.as_mut_ptr()) != -1,
            "pipe error: {}",
            io::Error::last_os_error()
        );
        let read = File::from_raw_fd(pipes[0]);
        let mut write = File::from_raw_fd(pipes[1]);
        let t = thread::spawn(move || {
            write.write_all(b"Hello!\n").unwrap();
            write.write_all(b"Good bye!\n").unwrap();
            thread::sleep(Duration::from_millis(100));
        });

        let source = PollEvented2::new_with_handle(MyFile::new(read), &handle).unwrap();

        let reader = read_to_end(source, Vec::new());
        let (_, content) = t!(reader.wait());
        assert_eq!(&b"Hello!\nGood bye!\n"[..], &content[..]);
        t.join().unwrap();
    }
}
