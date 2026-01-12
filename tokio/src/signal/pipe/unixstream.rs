use mio::net::UnixStream;
use std::io::{self, Read, Write};
use std::mem::ManuallyDrop;
use std::os::unix::io::{AsRawFd, FromRawFd};

pub(crate) struct Sender {
    inner: UnixStream,
}

#[derive(Debug)]
pub(crate) struct Receiver {
    inner: UnixStream,
}

impl Clone for Receiver {
    fn clone(&self) -> Self {
        let receiver_fd = self.inner.as_raw_fd();
        let original =
            ManuallyDrop::new(unsafe { std::os::unix::net::UnixStream::from_raw_fd(receiver_fd) });
        let inner = UnixStream::from_std(original.try_clone().expect("failed to clone UnixStream"));
        Self { inner }
    }
}

impl mio::event::Source for Receiver {
    fn register(
        &mut self,
        registry: &mio::Registry,
        token: mio::Token,
        interests: mio::Interest,
    ) -> io::Result<()> {
        self.inner.register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &mio::Registry,
        token: mio::Token,
        interests: mio::Interest,
    ) -> io::Result<()> {
        self.inner.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &mio::Registry) -> io::Result<()> {
        self.inner.deregister(registry)
    }
}

impl Sender {
    pub(crate) fn write(&self) {
        let _ = (&self.inner).write(&[1]);
    }
}

impl Receiver {
    pub(crate) fn read(&mut self) -> libc::c_int {
        // Drain the pipe completely so we can receive a new readiness event
        // if another signal has come in.
        let mut buf = [0; 128];
        #[allow(clippy::unused_io_amount)]
        loop {
            match self.inner.read(&mut buf) {
                Ok(0) => panic!("EOF on self-pipe"),
                Ok(_) => continue, // Keep reading
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("Bad read on self-pipe: {e}"),
            }
        }
        0
    }
}

pub(crate) fn channel() -> (Sender, Receiver) {
    let (sender, receiver) = UnixStream::pair().expect("failed to create UnixStream");
    (Sender { inner: sender }, Receiver { inner: receiver })
}

pub(crate) struct OsExtraData {
    sender: Sender,
    receiver: Receiver,
}

impl Default for OsExtraData {
    fn default() -> Self {
        let (sender, receiver) = channel();
        Self { sender, receiver }
    }
}

impl OsExtraData {
    pub(crate) fn receiver(&self) -> Receiver {
        self.receiver.clone()
    }

    pub(crate) fn sender(&self) -> &Sender {
        &self.sender
    }
}
