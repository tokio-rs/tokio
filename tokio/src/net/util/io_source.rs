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

macro_rules! poll_readiness {
    ($me:expr, $cache:ident, $mask:ident, $take:ident, $poll:expr) => {{
        let mut cached = $me.$cache.load(Relaxed);
        let mut current = $mask & cached;

        // If readiness is currently empty, loop until there is readiness;
        // otherwise consume pending events and return the new readiness.
        if current.is_empty() {
            loop {
                let new = match $poll? {
                    Poll::Ready(v) => v,
                    Poll::Pending => return Poll::Pending,
                };
                cached |= new.as_usize();

                $me.$cache.store(cached, Relaxed);

                current |= $mask & new;

                if !current.is_empty() {
                    return Poll::Ready(Ok(current));
                }
            }
        } else {
            if let Some(readiness) = $me.registration.$take()? {
                cached |= readiness.as_usize();
                $me.$cache.store(cached, Relaxed);
            }

            Poll::Ready(Ok(cached.into()))
        }
    }};
}

macro_rules! clear_readiness {
    ($me:expr, $cache:ident, $mask:expr, $poll:expr, $waker:expr) => {{
        $me.$cache.fetch_and(!$mask.as_usize(), Relaxed);
        if $poll?.is_ready() {
            // Notify the current task
            $waker.wake_by_ref()
        }
        Ok(())
    }};
}

/// TODO
pub struct IoSource<S: mio::event::Source> {
    io: Option<S>,
    registration: Registration,
    read_readiness: AtomicUsize,
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
            registration,
            read_readiness: AtomicUsize::default(),
            write_readiness: AtomicUsize::default(),
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
        self.registration.deregister(&io)?;
        Ok(io)
    }

    /// TODO
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        let mask = Readiness::readable() | Readiness::readable();
        poll_readiness!(
            self,
            read_readiness,
            mask,
            take_read_ready,
            self.registration.poll_read_ready(cx)
        )
    }

    /// TODO
    pub fn clear_read_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        clear_readiness!(
            self,
            read_readiness,
            Readiness::readable(),
            self.registration.poll_read_ready(cx),
            cx.waker()
        )
    }

    /// TODO
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Readiness>> {
        let mask = Readiness::writable() | Readiness::write_closed();
        poll_readiness!(
            self,
            write_readiness,
            mask,
            take_write_ready,
            self.registration.poll_write_ready(cx)
        )
    }

    /// TODO
    pub fn clear_write_ready(&self, cx: &mut Context<'_>) -> io::Result<()> {
        clear_readiness!(
            self,
            write_readiness,
            Readiness::writable(),
            self.registration.poll_write_ready(cx),
            cx.waker()
        )
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
            let _ = self.registration.deregister(&io);
        }
    }
}
