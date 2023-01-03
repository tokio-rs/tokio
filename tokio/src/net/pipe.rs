//! Tokio support for Unix pipes.

use mio::unix::pipe as mio_pipe;
use std::convert::TryFrom;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::prelude::OpenOptionsExt;
use std::path::Path;
use std::pin::Pin;
use std::process::{ChildStderr, ChildStdin, ChildStdout};
use std::task::{Context, Poll};

use crate::io::interest::Interest;
use crate::io::{AsyncRead, AsyncWrite, PollEvented, ReadBuf};

pub fn new() -> io::Result<(Sender, Receiver)> {
    let (tx, rx) = mio_pipe::new()?;
    Ok((Sender::from_mio(tx)?, Receiver::from_mio(rx)?))
}

cfg_net_unix! {
    #[derive(Debug)]
    pub struct Sender {
        io: PollEvented<mio_pipe::Sender>,
    }
}

impl Sender {
    pub fn open<P>(path: P) -> io::Result<Sender>
    where
        P: AsRef<Path>,
    {
        Sender::open_with_read_access(path.as_ref(), false)
    }

    pub fn open_dangling<P>(path: P) -> io::Result<Sender>
    where
        P: AsRef<Path>,
    {
        Sender::open_with_read_access(path.as_ref(), true)
    }

    fn open_with_read_access(path: &Path, read_access: bool) -> io::Result<Sender> {
        let file = OpenOptions::new()
            .read(read_access)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        if file.metadata()?.file_type().is_fifo() {
            let raw_fd = file.into_raw_fd();
            // Safety: We just created the raw fd from a valid fifo file.
            let pipe = unsafe { mio_pipe::Sender::from_raw_fd(raw_fd) };
            Sender::from_mio(pipe)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "file is not a fifo"))
        }
    }

    fn from_mio(pipe: mio_pipe::Sender) -> io::Result<Sender> {
        let io = PollEvented::new_with_interest(pipe, Interest::WRITABLE)?;
        Ok(Sender { io })
    }

    pub async fn writable(&self) -> io::Result<()> {
        self.io.registration().readiness(Interest::WRITABLE).await?;
        Ok(())
    }

    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.io.registration().poll_write_ready(cx).map_ok(|_| ())
    }

    pub fn try_write(&self, buf: &[u8]) -> io::Result<usize> {
        self.io
            .registration()
            .try_io(Interest::WRITABLE, || (&*self.io).write(buf))
    }

    pub fn try_write_vectored(&self, buf: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.io
            .registration()
            .try_io(Interest::WRITABLE, || (&*self.io).write_vectored(buf))
    }
}

impl AsyncWrite for Sender {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write_vectored(cx, bufs)
    }

    // TODO
    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl TryFrom<ChildStdin> for Sender {
    type Error = io::Error;
    fn try_from(stdin: ChildStdin) -> io::Result<Sender> {
        Sender::from_mio(stdin.into())
    }
}

impl AsRawFd for Sender {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}

cfg_net_unix! {
    #[derive(Debug)]
    pub struct Receiver {
        io: PollEvented<mio_pipe::Receiver>,
    }
}

impl Receiver {
    pub fn open<P>(path: P) -> io::Result<Receiver>
    where
        P: AsRef<Path>,
    {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;
        if file.metadata()?.file_type().is_fifo() {
            let raw_fd = file.into_raw_fd();
            // Safety: We just created the raw fd from a valid fifo file.
            let pipe = unsafe { mio_pipe::Receiver::from_raw_fd(raw_fd) };
            Receiver::from_mio(pipe)
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "file is not a fifo"))
        }
    }

    fn from_mio(pipe: mio_pipe::Receiver) -> io::Result<Receiver> {
        let io = PollEvented::new_with_interest(pipe, Interest::READABLE)?;
        Ok(Receiver { io })
    }

    pub async fn readable(&self) -> io::Result<()> {
        self.io.registration().readiness(Interest::READABLE).await?;
        Ok(())
    }

    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.io.registration().poll_read_ready(cx).map_ok(|_| ())
    }

    pub fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.io
            .registration()
            .try_io(Interest::READABLE, || (&*self.io).read(buf))
    }

    pub fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.io
            .registration()
            .try_io(Interest::READABLE, || (&*self.io).read_vectored(bufs))
    }
}

impl AsyncRead for Receiver {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // TODO
        // Safety: `mio_pipe::Receiver` uses a `std::fs::File::read` underneath,
        // which correctly handles reads into uninitialized memory.
        unsafe { self.io.poll_read(cx, buf) }
    }
}

impl TryFrom<ChildStdout> for Receiver {
    type Error = io::Error;
    fn try_from(stdout: ChildStdout) -> io::Result<Receiver> {
        Receiver::from_mio(stdout.into())
    }
}

impl TryFrom<ChildStderr> for Receiver {
    type Error = io::Error;
    fn try_from(stderr: ChildStderr) -> io::Result<Receiver> {
        Receiver::from_mio(stderr.into())
    }
}

impl AsRawFd for Receiver {
    fn as_raw_fd(&self) -> RawFd {
        self.io.as_raw_fd()
    }
}
