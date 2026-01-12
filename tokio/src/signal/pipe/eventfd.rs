use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

use mio::{event, unix::SourceFd};

#[derive(Debug)]
pub(crate) struct Sender {
    fd: OwnedFd,
}

#[derive(Debug)]
pub(crate) struct Receiver {
    fd: OwnedFd,
}

impl event::Source for Receiver {
    fn register(
        &mut self,
        registry: &mio::Registry,
        token: mio::Token,
        interests: mio::Interest,
    ) -> io::Result<()> {
        SourceFd(&self.fd.as_raw_fd()).register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &mio::Registry,
        token: mio::Token,
        interests: mio::Interest,
    ) -> io::Result<()> {
        SourceFd(&self.fd.as_raw_fd()).reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &mio::Registry) -> io::Result<()> {
        SourceFd(&self.fd.as_raw_fd()).deregister(registry)
    }
}

impl Sender {
    pub(crate) fn new() -> Self {
        let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if fd < 0 {
            panic!("eventfd failed: {}", io::Error::last_os_error());
        }
        Sender {
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        }
    }

    pub(crate) fn receiver(&self) -> Receiver {
        Receiver {
            fd: self.fd.try_clone().unwrap(),
        }
    }
}

impl Sender {
    pub(crate) fn write(&self) {
        unsafe {
            libc::eventfd_write(self.fd.as_raw_fd(), 1);
        }
    }
}

impl Receiver {
    pub(crate) fn read(&mut self) -> libc::c_int {
        let fd = &self.fd;
        let mut value: libc::eventfd_t = 0;

        unsafe { libc::eventfd_read(fd.as_raw_fd(), &mut value as *mut libc::eventfd_t) }
    }
}

pub(crate) struct OsExtraData {
    sender: Sender,
}

impl Default for OsExtraData {
    fn default() -> Self {
        let sender = Sender::new();
        Self { sender }
    }
}

impl OsExtraData {
    pub(crate) fn receiver(&self) -> Receiver {
        self.sender.receiver()
    }

    pub(crate) fn sender(&self) -> &Sender {
        &self.sender
    }
}
