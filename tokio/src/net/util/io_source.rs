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

/// TODO
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
    /// TODO
    pub fn new(io: S) -> io::Result<Self> {
        let registration = Registration::new(&io)?;
        Ok(Self {
            io: Some(io),
            inner: Inner {
                registration,
                read_readiness: AtomicUsize::default(),
                write_readiness: AtomicUsize::default(),
            },
        })
    }

    /// TODO
    pub fn get_ref(&self) -> &S {
        self.io.as_ref().unwrap()
    }

    /// TODO
    pub fn get_mut(&mut self) -> &mut S {
        self.io.as_mut().unwrap()
    }

    /// TODO
    pub fn into_inner(mut self) -> io::Result<S> {
        let io = self.io.take().unwrap();
        self.inner.registration.deregister(&io)?;
        Ok(io)
    }

    /// TODO
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        self.inner.poll_read_ready(cx)
    }

    /// TODO
    pub fn clear_read_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        self.inner
            .read_readiness
            .fetch_and(!Readiness::readable().as_usize(), Relaxed);
        if self.poll_read_ready(cx)?.is_ready() {
            // Notify the current task
            cx.waker().wake_by_ref();
        }
        Ok(())
    }

    /// TODO
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        self.inner.poll_write_ready(cx)
    }

    /// TODO
    pub fn clear_write_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        self.inner
            .read_readiness
            .fetch_and(!Readiness::writable().as_usize(), Relaxed);
        if self.poll_write_ready(cx)?.is_ready() {
            // Notify the current task
            cx.waker().wake_by_ref();
        }
        Ok(())
    }
}

impl Inner {
    fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        // Load cached readiness and see if it matches any bits
        let mut cached = self.read_readiness.load(Relaxed);
        let mut current = Readiness::from_usize(cached) & Readiness::readable();

        if current.is_empty() {
            loop {
                let readiness = match self.registration.poll_read_ready(cx)? {
                    Poll::Ready(v) => v,
                    Poll::Pending => return Poll::Pending,
                };
                cached |= readiness.as_usize();

                self.read_readiness.store(cached, Relaxed);

                current |= readiness;

                if !current.is_empty() {
                    return Poll::Ready(Ok(current));
                }
            }
        } else {
            if let Some(readiness) = self.registration.take_read_ready()? {
                cached |= readiness.as_usize();
                self.read_readiness.store(cached, Relaxed);
            }

            Poll::Ready(Ok(Readiness::from_usize(cached)))
        }
    }

    fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        // Load cached readiness and see if it matches any bits
        let mut cached = self.write_readiness.load(Relaxed);
        let mut current = Readiness::from_usize(cached) & Readiness::writable();

        if current.is_empty() {
            loop {
                let readiness = match self.registration.poll_write_ready(cx)? {
                    Poll::Ready(v) => v,
                    Poll::Pending => return Poll::Pending,
                };
                cached |= readiness.as_usize();

                self.write_readiness.store(cached, Relaxed);

                current |= readiness;

                if !current.is_empty() {
                    return Poll::Ready(Ok(current));
                }
            }
        } else {
            if let Some(readiness) = self.registration.take_write_ready()? {
                cached |= readiness.as_usize();
                self.write_readiness.store(cached, Relaxed);
            }

            Poll::Ready(Ok(Readiness::from_usize(cached)))
        }
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
