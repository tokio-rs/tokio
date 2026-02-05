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
    pub(crate) fn write(&self) -> std::io::Result<usize> {
        (&self.inner).write(&[1])
    }
}

impl Receiver {
    pub(crate) fn read(&mut self) -> std::io::Result<libc::c_int> {
        // Drain the pipe completely so we can receive a new readiness event
        // if another signal has come in.
        let mut buf = [0; 128];
        #[allow(clippy::unused_io_amount)]
        loop {
            match self.inner.read(&mut buf) {
                Ok(0) => panic!("EOF on self-pipe"),
                Ok(_) => continue, // Keep reading
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(0)
    }
}

pub(crate) fn channel() -> std::io::Result<(Sender, Receiver)> {
    let (sender, receiver) = UnixStream::pair()?;
    Ok((Sender { inner: sender }, Receiver { inner: receiver }))
}

pub(crate) struct OsExtraData {
    sender: Sender,
    receiver: Receiver,
}

impl OsExtraData {
    pub(crate) fn new() -> std::io::Result<Self> {
        let (sender, receiver) = channel()?;
        Ok(Self { sender, receiver })
    }
}

impl OsExtraData {
    pub(crate) fn receiver(&self) -> std::io::Result<Receiver> {
        let receiver_fd = self.receiver.inner.as_raw_fd();
        // SAFETY: fd owned by receiver is opened
        let original =
            ManuallyDrop::new(unsafe { std::os::unix::net::UnixStream::from_raw_fd(receiver_fd) });
        let inner = UnixStream::from_std(original.try_clone()?);
        Ok(Receiver { inner })
    }

    pub(crate) fn sender(&self) -> &Sender {
        &self.sender
    }
}
