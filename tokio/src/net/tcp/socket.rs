use crate::net::{TcpListener, TcpStream};

use std::fmt;
use std::io;
use std::net::SocketAddr;

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd, FromRawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};

/// TODO: Dox
pub struct TcpSocket {
    inner: mio::net::TcpSocket,
}

impl TcpSocket {
    /// TODO
    pub fn new_v4() -> io::Result<TcpSocket> {
        let inner = mio::net::TcpSocket::new_v4()?;
        Ok(TcpSocket { inner })
    }

    /// TODO
    pub fn new_v6() -> io::Result<TcpSocket> {
        let inner = mio::net::TcpSocket::new_v6()?;
        Ok(TcpSocket { inner })
    }

    /// TODO
    pub fn set_reuseaddr(&self, reuseaddr: bool) -> io::Result<()> {
        self.inner.set_reuseaddr(reuseaddr)
    }

    /// TODO
    pub fn bind(&self, addr: SocketAddr) -> io::Result<()> {
        self.inner.bind(addr)
    }

    /// TODO
    pub async fn connect(self, addr: SocketAddr) -> io::Result<TcpStream> {
        let mio = self.inner.connect(addr)?;
        TcpStream::connect_mio(mio).await
    }

    /// TODO
    pub fn listen(self, backlog: u32) -> io::Result<TcpListener> {
        let mio = self.inner.listen(backlog)?;
        TcpListener::new(mio)
    }
}

impl fmt::Debug for TcpSocket {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(fmt)
    }
}

#[cfg(unix)]
impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

#[cfg(unix)]
impl FromRawFd for TcpSocket {
    /// Converts a `RawFd` to a `TcpSocket`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> TcpSocket {
        let inner = mio::net::TcpSocket::from_raw_fd(fd);
        TcpSocket { inner }
    }
}

#[cfg(windows)]
impl IntoRawSocket for TcpSocket {
    fn into_raw_socket(self) -> RawSocket {
        self.inner.into_raw_socket()
    }
}

#[cfg(windows)]
impl AsRawSocket for TcpSocket {
    fn as_raw_socket(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

#[cfg(windows)]
impl FromRawSocket for TcpSocket {
    /// Converts a `RawSocket` to a `TcpStream`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_socket(socket: RawSocket) -> TcpSocket {
        let sys = mio::net::TcpSocket::from_raw_socket(socket);
        TcpSocket { sys }
    }
}
