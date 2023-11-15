use crate::{
    io::{interest::Interest, PollEvented},
    process::{imp::orphan::Wait, kill::Kill},
    runtime::Handle,
};

use libc::{syscall, SYS_pidfd_open, __errno_location, ENOSYS, PIDFD_NONBLOCK};
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
    sync::atomic::{AtomicBool, Ordering::Relaxed},
    task::{Context, Poll},
};

#[derive(Debug)]
struct Pidfd {
    fd: File,
}

impl Pidfd {
    fn open(pid: u32) -> Option<Pidfd> {
        // Store false (0) to reduce executable size
        static NO_PIDFD_SUPPORT: AtomicBool = AtomicBool::new(false);

        if NO_PIDFD_SUPPORT.load(Relaxed) {
            return None;
        }

        unsafe {
            let fd = syscall(SYS_pidfd_open, pid, PIDFD_NONBLOCK);
            if fd == -1 {
                let errno = *__errno_location();

                if errno == ENOSYS {
                    NO_PIDFD_SUPPORT.store(true, Relaxed)
                }

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
    pub(crate) fn new(inner: W) -> Result<Self, (Option<io::Error>, W)> {
        if let Some(pidfd) = Pidfd::open(inner.id()) {
            match PollEvented::new_with_interest(pidfd, Interest::READABLE) {
                Ok(pidfd) => Ok(Self(Some(PidfdReaperInner { pidfd, inner }))),
                Err(io_error) => Err((Some(io_error), inner)),
            }
        } else {
            Err((None, inner))
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

#[cfg(all(test, not(loom), not(miri)))]
mod test {
    use super::*;
    use crate::runtime::{Builder as RuntimeBuilder, Runtime};
    use std::process::{Command, Output};

    fn create_runtime() -> Runtime {
        RuntimeBuilder::new_current_thread()
            .enable_io()
            .build()
            .unwrap()
    }

    fn run_test(fut: impl Future<Output = ()>) {
        create_runtime().block_on(fut)
    }

    fn is_pidfd_available() -> bool {
        let Output { stdout, status, .. } = Command::new("uname").arg("-r").output().unwrap();
        assert!(status.success());
        let stdout = String::from_utf8_lossy(&stdout);

        let mut kernel_version_iter = stdout.split_once('-').unwrap().0.split('.');
        let major: u32 = kernel_version_iter.next().unwrap().parse().unwrap();
        let minor: u32 = kernel_version_iter.next().unwrap().parse().unwrap();

        major >= 6 || (major == 5 && minor >= 10)
    }

    #[test]
    fn test_pidfd_reaper_poll() {
        if !is_pidfd_available() {
            eprintln!("pidfd is not available on this linux kernel, skip this test");
            return;
        }

        run_test(async {
            let child = Command::new("true").spawn().unwrap();
            let pidfd_reaper = PidfdReaper::new(child).unwrap();

            let exit_status = pidfd_reaper.await.unwrap();
            assert!(exit_status.success());
        });
    }

    #[test]
    fn test_pidfd_reaper_kill() {
        if !is_pidfd_available() {
            eprintln!("pidfd is not available on this linux kernel, skip this test");
            return;
        }

        run_test(async {
            let child = Command::new("sleep").arg("1800").spawn().unwrap();
            let mut pidfd_reaper = PidfdReaper::new(child).unwrap();

            pidfd_reaper.kill().unwrap();

            let exit_status = pidfd_reaper.await.unwrap();
            assert!(!exit_status.success());
        });
    }

    #[test]
    fn test_pidfd_reaper_drop() {
        if !is_pidfd_available() {
            eprintln!("pidfd is not available on this linux kernel, skip this test");
            return;
        }

        run_test(async {
            let child = Command::new("true").spawn().unwrap();
            let _pidfd_reaper = PidfdReaper::new(child).unwrap();
        });
    }
}
