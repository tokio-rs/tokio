use ucred::{self, UCred};

use tokio_io::{AsyncRead, AsyncWrite};
use tokio_reactor::{Handle, PollEvented};

use bytes::{Buf, BufMut};
use futures::{Async, Future, Poll};
use iovec::{self, IoVec};
use libc;
use mio::Ready;
use mio_uds;

use std::fmt;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;

/// A structure representing a connected Unix socket.
///
/// This socket can be connected directly with `UnixStream::connect` or accepted
/// from a listener with `UnixListener::incoming`. Additionally, a pair of
/// anonymous Unix sockets can be created with `UnixStream::pair`.
pub struct UnixStream {
    io: PollEvented<mio_uds::UnixStream>,
}

/// Future returned by `UnixStream::connect` which will resolve to a
/// `UnixStream` when the stream is connected.
#[derive(Debug)]
pub struct ConnectFuture {
    inner: State,
}

#[derive(Debug)]
enum State {
    Waiting(UnixStream),
    Error(io::Error),
    Empty,
}

impl UnixStream {
    /// Connects to the socket named by `path`.
    ///
    /// This function will create a new Unix socket and connect to the path
    /// specified, associating the returned stream with the default event loop's
    /// handle.
    pub fn connect<P>(path: P) -> ConnectFuture
    where
        P: AsRef<Path>,
    {
        let res = mio_uds::UnixStream::connect(path)
            .map(UnixStream::new);

        let inner = match res {
            Ok(stream) => State::Waiting(stream),
            Err(e) => State::Error(e),
        };

        ConnectFuture { inner }
    }

    /// Consumes a `UnixStream` in the standard library and returns a
    /// nonblocking `UnixStream` from this crate.
    ///
    /// The returned stream will be associated with the given event loop
    /// specified by `handle` and is ready to perform I/O.
    pub fn from_std(stream: net::UnixStream, handle: &Handle) -> io::Result<UnixStream> {
        let stream = mio_uds::UnixStream::from_stream(stream)?;
        let io = PollEvented::new_with_handle(stream, handle)?;

        Ok(UnixStream { io })
    }

    /// Creates an unnamed pair of connected sockets.
    ///
    /// This function will create a pair of interconnected Unix sockets for
    /// communicating back and forth between one another. Each socket will
    /// be associated with the default event loop's handle.
    pub fn pair() -> io::Result<(UnixStream, UnixStream)> {
        let (a, b) = try!(mio_uds::UnixStream::pair());
        let a = UnixStream::new(a);
        let b = UnixStream::new(b);

        Ok((a, b))
    }

    pub(crate) fn new(stream: mio_uds::UnixStream) -> UnixStream {
        let io = PollEvented::new(stream);
        UnixStream { io }
    }

    /// Test whether this socket is ready to be read or not.
    pub fn poll_read_ready(&self, ready: Ready) -> Poll<Ready, io::Error> {
        self.io.poll_read_ready(ready)
    }

    /// Test whether this socket is ready to be written to or not.
    pub fn poll_write_ready(&self) -> Poll<Ready, io::Error> {
        self.io.poll_write_ready()
    }

    /// Returns the socket address of the local half of this connection.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the socket address of the remote half of this connection.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().peer_addr()
    }

    /// Returns effective credentials of the process which called `connect` or `pair`.
    pub fn peer_cred(&self) -> io::Result<UCred> {
        ucred::get_peer_cred(self)
    }

    /// Returns the value of the `SO_ERROR` option.
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.get_ref().take_error()
    }

    /// Shuts down the read, write, or both halves of this connection.
    ///
    /// This function will cause all pending and future I/O calls on the
    /// specified portions to immediately return with an appropriate value
    /// (see the documentation of `Shutdown`).
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.io.get_ref().shutdown(how)
    }
}

impl Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io.read(buf)
    }
}

impl Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.io.flush()
    }
}

impl AsyncRead for UnixStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        <&UnixStream>::read_buf(&mut &*self, buf)
    }
}

impl AsyncWrite for UnixStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        <&UnixStream>::shutdown(&mut &*self)
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        <&UnixStream>::write_buf(&mut &*self, buf)
    }
}

impl<'a> Read for &'a UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.io).read(buf)
    }
}

impl<'a> Write for &'a UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.io).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.io).flush()
    }
}

impl<'a> AsyncRead for &'a UnixStream {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }

    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        if let Async::NotReady = <UnixStream>::poll_read_ready(self, Ready::readable())? {
            return Ok(Async::NotReady);
        }
        unsafe {
            let r = read_ready(buf, self.as_raw_fd());
            if r == -1 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.clear_read_ready(Ready::readable())?;
                    Ok(Async::NotReady)
                } else {
                    Err(e)
                }
            } else {
                let r = r as usize;
                buf.advance_mut(r);
                Ok(r.into())
            }
        }
    }
}

impl<'a> AsyncWrite for &'a UnixStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }

    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, io::Error> {
        if let Async::NotReady = <UnixStream>::poll_write_ready(self)? {
            return Ok(Async::NotReady);
        }
        unsafe {
            let r = write_ready(buf, self.as_raw_fd());
            if r == -1 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.clear_write_ready()?;
                    Ok(Async::NotReady)
                } else {
                    Err(e)
                }
            } else {
                let r = r as usize;
                buf.advance(r);
                Ok(r.into())
            }
        }
    }
}

impl fmt::Debug for UnixStream {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}

impl Future for ConnectFuture {
    type Item = UnixStream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<UnixStream, io::Error> {
        use std::mem;

        match self.inner {
            State::Waiting(ref mut stream) => {
                if let Async::NotReady = stream.io.poll_write_ready()? {
                    return Ok(Async::NotReady)
                }

                if let Some(e) = try!(stream.io.get_ref().take_error()) {
                    return Err(e)
                }
            }
            State::Error(_) => {
                let e = match mem::replace(&mut self.inner, State::Empty) {
                    State::Error(e) => e,
                    _ => unreachable!(),
                };

                return Err(e)
            },
            State::Empty => panic!("can't poll stream twice"),
        }

        match mem::replace(&mut self.inner, State::Empty) {
            State::Waiting(stream) => Ok(Async::Ready(stream)),
            _ => unreachable!(),
        }
    }
}

unsafe fn read_ready<B: BufMut>(buf: &mut B, raw_fd: RawFd) -> isize {
    // The `IoVec` type can't have a 0-length size, so we create a bunch
    // of dummy versions on the stack with 1 length which we'll quickly
    // overwrite.
    let b1: &mut [u8] = &mut [0];
    let b2: &mut [u8] = &mut [0];
    let b3: &mut [u8] = &mut [0];
    let b4: &mut [u8] = &mut [0];
    let b5: &mut [u8] = &mut [0];
    let b6: &mut [u8] = &mut [0];
    let b7: &mut [u8] = &mut [0];
    let b8: &mut [u8] = &mut [0];
    let b9: &mut [u8] = &mut [0];
    let b10: &mut [u8] = &mut [0];
    let b11: &mut [u8] = &mut [0];
    let b12: &mut [u8] = &mut [0];
    let b13: &mut [u8] = &mut [0];
    let b14: &mut [u8] = &mut [0];
    let b15: &mut [u8] = &mut [0];
    let b16: &mut [u8] = &mut [0];
    let mut bufs: [&mut IoVec; 16] = [
        b1.into(),
        b2.into(),
        b3.into(),
        b4.into(),
        b5.into(),
        b6.into(),
        b7.into(),
        b8.into(),
        b9.into(),
        b10.into(),
        b11.into(),
        b12.into(),
        b13.into(),
        b14.into(),
        b15.into(),
        b16.into(),
    ];

    let n = buf.bytes_vec_mut(&mut bufs);
    read_ready_vecs(&mut bufs[..n], raw_fd)
}

unsafe fn read_ready_vecs(bufs: &mut [&mut IoVec], raw_fd: RawFd) -> isize {
    let iovecs = iovec::unix::as_os_slice_mut(bufs);

    libc::readv(raw_fd, iovecs.as_ptr(), iovecs.len() as i32)
}

unsafe fn write_ready<B: Buf>(buf: &mut B, raw_fd: RawFd) -> isize {
    // The `IoVec` type can't have a zero-length size, so create a dummy
    // version from a 1-length slice which we'll overwrite with the
    // `bytes_vec` method.
    static DUMMY: &[u8] = &[0];
    let iovec = <&IoVec>::from(DUMMY);
    let mut bufs = [
        iovec, iovec, iovec, iovec, iovec, iovec, iovec, iovec, iovec, iovec, iovec, iovec, iovec,
        iovec, iovec, iovec,
    ];

    let n = buf.bytes_vec(&mut bufs);
    write_ready_vecs(&bufs[..n], raw_fd)
}

unsafe fn write_ready_vecs(bufs: &[&IoVec], raw_fd: RawFd) -> isize {
    let iovecs = iovec::unix::as_os_slice(bufs);

    libc::writev(raw_fd, iovecs.as_ptr(), iovecs.len() as i32)
}
