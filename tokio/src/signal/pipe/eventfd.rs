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
    pub(crate) fn new() -> std::io::Result<Self> {
        // SAFETY: it's ok to call libc API
        let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Sender {
            // SAFETY: fd just opened by the above libc::eventfd
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        })
    }

    pub(crate) fn receiver(&self) -> std::io::Result<Receiver> {
        Ok(Receiver {
            fd: self.fd.try_clone()?,
        })
    }
}

impl Sender {
    pub(crate) fn write(&self) -> std::io::Result<usize> {
        // SAFETY: it's ok to call libc API
        let r = unsafe { libc::eventfd_write(self.fd.as_raw_fd(), 1) };
        if r == 0 {
            Ok(0)
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

impl Receiver {
    pub(crate) fn read(&mut self) -> std::io::Result<libc::c_int> {
        let fd = self.fd.as_raw_fd();
        let mut value: libc::eventfd_t = 0;

        // SAFETY: it's ok to call libc API
        let r = unsafe { libc::eventfd_read(fd, &mut value as *mut libc::eventfd_t) };
        if r == 0 {
            Ok(0)
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

pub(crate) struct OsExtraData {
    sender: Sender,
}

impl OsExtraData {
    pub(crate) fn new() -> std::io::Result<Self> {
        Sender::new().map(|sender| Self { sender })
    }
}

impl OsExtraData {
    pub(crate) fn receiver(&self) -> std::io::Result<Receiver> {
        self.sender.receiver()
    }

    pub(crate) fn sender(&self) -> &Sender {
        &self.sender
    }
}
