use crate::{
    io::{interest::Interest, PollEvented},
    process::{imp::orphan::Wait, kill::Kill},
    runtime::Handle,
};

use libc::{syscall, SYS_pidfd_open, PIDFD_NONBLOCK};
use mio::{event::Source, unix::SourceFd};
use std::{
    fs::File,
    future::Future,
    io,
    marker::Unpin,
    ops::Deref,
    os::unix::io::{AsRawFd, FromRawFd, RawFd},
    pin::Pin,
    process::ExitStatus,
    task::{Context, Poll},
};

#[derive(Debug)]
struct Pidfd {
    fd: File,
}

impl Pidfd {
    fn open(pid: u32) -> Option<Pidfd> {
        unsafe {
            let fd = syscall(SYS_pidfd_open, pid, PIDFD_NONBLOCK);
            if fd == -1 {
                None
            } else {
                Some(Pidfd {
                    fd: File::from_raw_fd(fd as i32),
                })
            }
        }
    }
}

impl AsRawFd for Pidfd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl Source for Pidfd {
    fn register(
        &mut self,
        registry: &mio::Registry,
        token: mio::Token,
        interest: mio::Interest,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).register(registry, token, interest)
    }

    fn reregister(
        &mut self,
        registry: &mio::Registry,
        token: mio::Token,
        interest: mio::Interest,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).reregister(registry, token, interest)
    }

    fn deregister(&mut self, registry: &mio::Registry) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).deregister(registry)
    }
}

#[derive(Debug)]
struct PidfdReaperInner<W>
where
    W: Unpin,
{
    inner: W,
    pidfd: PollEvented<Pidfd>,
}

impl<W> Future for PidfdReaperInner<W>
where
    W: Wait + Unpin,
{
    type Output = io::Result<ExitStatus>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);

        ready!(this.pidfd.poll_read_ready(cx))?;
        Poll::Ready(Ok(this
            .inner
            .try_wait()?
            .expect("pidfd is ready to read, the process should have exited")))
    }
}

#[derive(Debug)]
pub(crate) struct PidfdReaper<W: Wait + Send + Sync + Unpin + 'static>(Option<PidfdReaperInner<W>>);

impl<W> Deref for PidfdReaper<W>
where
    W: Wait + Send + Sync + Unpin + 'static,
{
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.0.as_ref().expect("inner has gone away").inner
    }
}

impl<W> PidfdReaper<W>
where
    W: Wait + Send + Sync + Unpin + 'static,
{
    pub(crate) fn new(inner: W) -> io::Result<Option<Self>> {
        if let Some(pidfd) = Pidfd::open(inner.id()) {
            Ok(Some(Self(Some(PidfdReaperInner {
                pidfd: PollEvented::new_with_interest(pidfd, Interest::READABLE)?,
                inner,
            }))))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn inner_mut(&mut self) -> &mut W {
        &mut self.0.as_mut().expect("inner has gone away").inner
    }
}

impl<W> Future for PidfdReaper<W>
where
    W: Wait + Send + Sync + Unpin + 'static,
{
    type Output = io::Result<ExitStatus>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(
            Pin::into_inner(self)
                .0
                .as_mut()
                .expect("inner has gone away"),
        )
        .poll(cx)
    }
}

impl<W> Kill for PidfdReaper<W>
where
    W: Wait + Send + Sync + Unpin + Kill + 'static,
{
    fn kill(&mut self) -> io::Result<()> {
        self.inner_mut().kill()
    }
}

impl<W> Drop for PidfdReaper<W>
where
    W: Wait + Send + Sync + Unpin + 'static,
{
    fn drop(&mut self) {
        let mut reaper_inner = self.0.take().expect("inner has gone away");
        if let Ok(Some(_)) = reaper_inner.inner.try_wait() {
            return;
        }

        Handle {
            inner: reaper_inner.pidfd.scheduler_handle().clone(),
        }
        .spawn(async move {
            let _ = reaper_inner.await;
        });
    }
}
