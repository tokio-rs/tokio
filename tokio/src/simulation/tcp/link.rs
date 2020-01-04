//! [`Link`] provides a double-sided [`AsyncRead`] + [`AsyncWrite`]
//! which can be used to simulate a TCP connection in-memory.
//!
//! [`AsyncRead`](tokio::io::AsyncRead)
//! [`AsyncWrite`](tokio::io::AsyncWrite)
//! [`Link`]:struct.Link.html
use crate::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};
use bytes::{Buf, Bytes};
use futures_core::Stream;
use std::{future::Future, io, pin::Pin, task::Context, task::Poll};

/// Link between two TcpStreams.
#[derive(Debug)]
pub(crate) struct Link {
    tx: mpsc::Sender<Bytes>,
    rx: mpsc::Receiver<Bytes>,
    staged: Option<Bytes>,
}

impl Link {
    fn new(tx: mpsc::Sender<Bytes>, rx: mpsc::Receiver<Bytes>) -> Self {
        Self {
            tx,
            rx,
            staged: None,
        }
    }
    /// Construct a new [`Link`] pair.
    ///
    /// The returned [`Link`] pair can be used to communicate via [`AsyncRead`] and
    /// [`AsyncWrite`].
    ///
    /// [`AsyncRead`]:tokio::io::AsyncRead
    /// [`AsyncWrite`]:tokio::io::AsyncWrite
    /// [`Link`]:struct.Link.html
    pub(crate) fn new_pair() -> (Self, Self) {
        let (ltx, lrx) = mpsc::channel(8);
        let (rtx, rrx) = mpsc::channel(8);
        (Self::new(ltx, rrx), Self::new(rtx, lrx))
    }

    /// Read any staged data from this side of the [`Link`] into `dst`.
    ///
    /// [`Link`]:struct.Link.html
    fn read_staged(&mut self, dst: &mut [u8]) -> Option<usize> {
        if let Some(mut bytes) = self.staged.take() {
            debug_assert!(!bytes.is_empty(), "staged bytes should not be empty");
            let to_write = std::cmp::min(dst.len(), bytes.len());
            let mut b = bytes.split_to(to_write);
            b.copy_to_slice(&mut dst[..to_write]);
            if !bytes.is_empty() {
                self.staged.replace(bytes);
            }
            Some(to_write)
        } else {
            None
        }
    }

    pub fn poll_peek(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        todo!("implement peek")
    }
}

impl AsyncRead for Link {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            if let Some(bytes_read) = self.read_staged(buf) {
                return Poll::Ready(Ok(bytes_read));
            }
            let stream = Pin::new(&mut self.rx);
            match ready!(stream.poll_next(cx)) {
                Some(new) => {
                    self.staged.replace(new);
                }
                None => return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
            }
        }
    }
}

impl AsyncWrite for Link {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let size = buf.len();
        let buf: Bytes = buf.to_owned().into();
        let send = self.tx.send(buf);
        let mut send = send;
        let send = unsafe { Pin::new_unchecked(&mut send) };
        match ready!(send.poll(cx)) {
            Ok(()) => Poll::Ready(Ok(size)),
            Err(_) => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}
