use crate::io::{AsyncRead, AsyncWrite};
use crate::net::driver::{Readiness, Registration};

use futures_core::ready;
use mio;
use std::fmt;
use std::io::{self, Read, Write};
use std::marker::Unpin;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::task::{Context, Poll};

pub struct IoSource<S: mio::event::Source> {
    io: Option<S>,
    inner: Inner,
}

struct Inner {
    registration: Registration,

    /// Currently visible read readiness
    read_readiness: AtomicUsize,

    /// Currently visible write readiness
    write_readiness: AtomicUsize,
}

impl<S> IoSource<S>
where
    S: mio::event::Source,
{
    pub fn new(io: S) -> io::Result<Self> {
        unimplemented!()
    }

    pub fn get_ref(&self) -> &S {
        self.io.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut S {
        self.io.as_mut().unwrap()
    }

    pub fn into_inner(mut self) -> io::Result<S> {
        let io = self.io.take().unwrap();
        self.inner.registration.deregister(&io)?;
        Ok(io)
    }

    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        self.poll_ready(
            cx,
            Inner::get_read_readiness,
            Registration::poll_read_ready,
            Readiness::readable(),
        )
    }
    pub fn clear_read_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        unimplemented!()
    }

    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        self.poll_ready(
            cx,
            Inner::get_write_readiness,
            Registration::poll_write_ready,
            Readiness::writable(),
        )
    }

    pub fn clear_write_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        unimplemented!()
    }

    fn poll_ready<C, P>(
        &self,
        cx: &mut Context<'_>,
        cached_readiness: C,
        poll_readiness: P,
        readiness: Readiness,
    ) -> Poll<io::Result<Readiness>>
    where
        C: for<'r> FnOnce(&'r Inner) -> Readiness,
        P: for<'r> Fn(&'r Registration, &mut Context<'_>) -> Poll<io::Result<Readiness>>,
    {
        let cached = cached_readiness(&self.inner);
        let readiness = cached & readiness;

        if readiness.is_empty() {
            loop {
                let ready = match poll_readiness(&self.inner.registration, cx) {
                    Poll::Ready(v) => v,
                    Poll::Pending => return Poll::Pending,
                };
            }
        }

        unimplemented!()
    }
}

impl Inner {
    fn get_read_readiness(&self) -> Readiness {
        let cached = self.read_readiness.load(Relaxed);
        Readiness::from_usize(cached)
    }

    fn get_write_readiness(&self) -> Readiness {
        let cached = self.write_readiness.load(Relaxed);
        Readiness::from_usize(cached)
    }
}

// ===== Read / Write impls =====

impl<S> AsyncRead for IoSource<S>
where
    S: mio::event::Source + Read + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.poll_read_ready(cx))?;

        let r = (*self).get_mut().read(buf);

        if is_wouldblock(&r) {
            self.clear_read_ready(cx)?;
            return Poll::Pending;
        }

        Poll::Ready(r)
    }
}

impl<S> AsyncWrite for IoSource<S>
where
    S: mio::event::Source + Write + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.poll_write_ready(cx))?;

        let r = (*self).get_mut().write(buf);

        if is_wouldblock(&r) {
            self.clear_write_ready(cx)?;
            return Poll::Pending;
        }

        Poll::Ready(r)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.poll_write_ready(cx))?;

        let r = (*self).get_mut().flush();

        if is_wouldblock(&r) {
            self.clear_write_ready(cx)?;
            return Poll::Pending;
        }

        Poll::Ready(r)
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn is_wouldblock<T>(r: &io::Result<T>) -> bool {
    match *r {
        Ok(_) => false,
        Err(ref e) => e.kind() == io::ErrorKind::WouldBlock,
    }
}

impl<S: mio::event::Source + fmt::Debug> fmt::Debug for IoSource<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source").field("io", &self.io).finish()
    }
}

impl<S: mio::event::Source> Drop for IoSource<S> {
    fn drop(&mut self) {
        if let Some(io) = self.io.take() {
            // Ignore errors
            let _ = self.inner.registration.deregister(&io);
        }
    }
}
