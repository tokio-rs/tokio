//! Unix handling of child processes.
//!
//! Right now the only "fancy" thing about this is how we implement the
//! `Future` implementation on `Child` to get the exit status. Unix offers
//! no way to register a child with epoll, and the only real way to get a
//! notification when a process exits is the SIGCHLD signal.
//!
//! Signal handling in general is *super* hairy and complicated, and it's even
//! more complicated here with the fact that signals are coalesced, so we may
//! not get a SIGCHLD-per-child.
//!
//! Our best approximation here is to check *all spawned processes* for all
//! SIGCHLD signals received. To do that we create a `Signal`, implemented in
//! the `tokio-net` crate, which is a stream over signals being received.
//!
//! Later when we poll the process's exit status we simply check to see if a
//! SIGCHLD has happened since we last checked, and while that returns "yes" we
//! keep trying.
//!
//! Note that this means that this isn't really scalable, but then again
//! processes in general aren't scalable (e.g. millions) so it shouldn't be that
//! bad in theory...

pub(crate) mod orphan;
use orphan::{OrphanQueue, OrphanQueueImpl, Wait};

mod reap;
use reap::Reaper;

#[cfg(all(target_os = "linux", feature = "rt"))]
mod pidfd_reaper;

use crate::io::interest::Interest;
use crate::io::{AsyncRead, AsyncWrite, PollEvented, ReadBuf};
use crate::process::kill::Kill;
use crate::process::SpawnedChild;
use crate::runtime::signal::Handle as SignalHandle;
use crate::signal::unix::{signal, Signal, SignalKind};

use mio::event::Source;
use mio::unix::pipe;
use std::fmt;
use std::fs::File;
use std::future::Future;
use std::io;
use std::os::unix::io::{AsRawFd, OwnedFd, RawFd};
use std::pin::Pin;
use std::process::{
    Child as StdChild, ChildStderr as StdChildStderr, ChildStdin as StdChildStdin,
    ChildStdout as StdChildStdout, ExitStatus, Stdio,
};
use std::task::Context;
use std::task::Poll;

impl Wait for StdChild {
    fn id(&self) -> u32 {
        self.id()
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.try_wait()
    }
}

impl Kill for StdChild {
    fn kill(&mut self) -> io::Result<()> {
        self.kill()
    }
}

cfg_not_has_const_mutex_new! {
    fn get_orphan_queue() -> &'static OrphanQueueImpl<StdChild> {
        use std::sync::OnceLock;

        static ORPHAN_QUEUE: OnceLock<OrphanQueueImpl<StdChild>> = OnceLock::new();

        ORPHAN_QUEUE.get_or_init(OrphanQueueImpl::new)
    }
}

cfg_has_const_mutex_new! {
    fn get_orphan_queue() -> &'static OrphanQueueImpl<StdChild> {
        static ORPHAN_QUEUE: OrphanQueueImpl<StdChild> = OrphanQueueImpl::new();

        &ORPHAN_QUEUE
    }
}

pub(crate) struct GlobalOrphanQueue;

impl fmt::Debug for GlobalOrphanQueue {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        get_orphan_queue().fmt(fmt)
    }
}

impl GlobalOrphanQueue {
    pub(crate) fn reap_orphans(handle: &SignalHandle) {
        get_orphan_queue().reap_orphans(handle);
    }
}

impl OrphanQueue<StdChild> for GlobalOrphanQueue {
    fn push_orphan(&self, orphan: StdChild) {
        get_orphan_queue().push_orphan(orphan);
    }
}

/// After the OS has already created a process, failures while wrapping it for
/// Tokio (stdio registration, pidfd, SIGCHLD listener) must not drop the
/// `StdChild` handle without reaping or enqueuing. Otherwise the process can
/// become an unreapable zombie until this process exits.
///
/// If `try_wait` reports that the child has already exited, it has been reaped
/// and nothing else is needed. Otherwise push onto the orphan queue so the
/// process driver can reap it later.
fn orphan_child_on_error<W, Q>(mut child: W, orphan_queue: Q, err: io::Error) -> io::Error
where
    W: Wait,
    Q: OrphanQueue<W>,
{
    match child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) | Err(_) => orphan_queue.push_orphan(child),
    }
    err
}

#[must_use = "futures do nothing unless polled"]
pub(crate) enum Child {
    SignalReaper(Reaper<StdChild, GlobalOrphanQueue, Signal>),
    #[cfg(all(target_os = "linux", feature = "rt"))]
    PidfdReaper(pidfd_reaper::PidfdReaper<StdChild, GlobalOrphanQueue>),
}

impl fmt::Debug for Child {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Child").field("pid", &self.id()).finish()
    }
}

pub(crate) fn build_child(mut child: StdChild) -> io::Result<SpawnedChild> {
    // Take every pipe before registering one. On a registration error the
    // remaining pipes drop here. Leaving them on the orphaned child keeps
    // the parent end open, so the child can block on a full pipe.
    let stdin_pipe = child.stdin.take();
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdin = match stdin_pipe.map(stdio).transpose() {
        Ok(io) => io,
        Err(e) => return Err(orphan_child_on_error(child, GlobalOrphanQueue, e)),
    };
    let stdout = match stdout_pipe.map(stdio).transpose() {
        Ok(io) => io,
        Err(e) => return Err(orphan_child_on_error(child, GlobalOrphanQueue, e)),
    };
    let stderr = match stderr_pipe.map(stdio).transpose() {
        Ok(io) => io,
        Err(e) => return Err(orphan_child_on_error(child, GlobalOrphanQueue, e)),
    };

    #[cfg(all(target_os = "linux", feature = "rt"))]
    match pidfd_reaper::PidfdReaper::new(child, GlobalOrphanQueue) {
        Ok(pidfd_reaper) => {
            return Ok(SpawnedChild {
                child: Child::PidfdReaper(pidfd_reaper),
                stdin,
                stdout,
                stderr,
            })
        }
        Err((Some(err), child)) => {
            return Err(orphan_child_on_error(child, GlobalOrphanQueue, err));
        }
        Err((None, child_returned)) => child = child_returned,
    }

    let signal = match signal(SignalKind::child()) {
        Ok(signal) => signal,
        Err(e) => return Err(orphan_child_on_error(child, GlobalOrphanQueue, e)),
    };

    Ok(SpawnedChild {
        child: Child::SignalReaper(Reaper::new(child, GlobalOrphanQueue, signal)),
        stdin,
        stdout,
        stderr,
    })
}

impl Child {
    pub(crate) fn id(&self) -> u32 {
        match self {
            Self::SignalReaper(signal_reaper) => signal_reaper.id(),
            #[cfg(all(target_os = "linux", feature = "rt"))]
            Self::PidfdReaper(pidfd_reaper) => pidfd_reaper.id(),
        }
    }

    fn std_child(&mut self) -> &mut StdChild {
        match self {
            Self::SignalReaper(signal_reaper) => signal_reaper.inner_mut(),
            #[cfg(all(target_os = "linux", feature = "rt"))]
            Self::PidfdReaper(pidfd_reaper) => pidfd_reaper.inner_mut(),
        }
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.std_child().try_wait()
    }
}

impl Kill for Child {
    fn kill(&mut self) -> io::Result<()> {
        self.std_child().kill()
    }
}

impl Future for Child {
    type Output = io::Result<ExitStatus>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::into_inner(self) {
            Self::SignalReaper(signal_reaper) => Pin::new(signal_reaper).poll(cx),
            #[cfg(all(target_os = "linux", feature = "rt"))]
            Self::PidfdReaper(pidfd_reaper) => Pin::new(pidfd_reaper).poll(cx),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ChildStdin {
    inner: PollEvented<pipe::Sender>,
}

#[derive(Debug)]
pub(crate) struct ChildStdout {
    inner: PollEvented<pipe::Receiver>,
}

pub(crate) type ChildStderr = ChildStdout;

impl TryFrom<StdChildStdin> for ChildStdin {
    type Error = io::Error;

    fn try_from(io: StdChildStdin) -> io::Result<Self> {
        let mut pipe = pipe::Sender::from(io);
        set_nonblocking(&mut pipe, true)?;
        PollEvented::new_with_interest(pipe, Interest::WRITABLE).map(|inner| Self { inner })
    }
}

impl TryFrom<StdChildStdout> for ChildStdout {
    type Error = io::Error;

    fn try_from(io: StdChildStdout) -> io::Result<Self> {
        let mut pipe = pipe::Receiver::from(io);
        set_nonblocking(&mut pipe, true)?;
        PollEvented::new_with_interest(pipe, Interest::READABLE).map(|inner| Self { inner })
    }
}

impl TryFrom<StdChildStderr> for ChildStdout {
    type Error = io::Error;

    fn try_from(io: StdChildStderr) -> io::Result<Self> {
        let mut pipe = pipe::Receiver::from(io);
        set_nonblocking(&mut pipe, true)?;
        PollEvented::new_with_interest(pipe, Interest::READABLE).map(|inner| Self { inner })
    }
}

fn convert_to_blocking_file<T>(io: PollEvented<T>) -> io::Result<File>
where
    T: Source + Into<OwnedFd>,
{
    let fd: OwnedFd = io.into_inner()?.into();
    let mut fd = File::from(fd);

    // Ensure that the fd to be inherited is set to *blocking* mode, as this
    // is the default that virtually all programs expect to have. Those
    // programs that know how to work with nonblocking stdio will know how to
    // change it to nonblocking mode.
    set_nonblocking(&mut fd, false)?;

    Ok(fd)
}

impl ChildStdin {
    pub(super) fn into_owned_fd(self) -> io::Result<OwnedFd> {
        convert_to_blocking_file(self.inner).map(OwnedFd::from)
    }

    pub(super) fn into_stdio(self) -> io::Result<Stdio> {
        convert_to_blocking_file(self.inner).map(Stdio::from)
    }
}

impl ChildStdout {
    pub(super) fn into_owned_fd(self) -> io::Result<OwnedFd> {
        convert_to_blocking_file(self.inner).map(OwnedFd::from)
    }

    pub(super) fn into_stdio(self) -> io::Result<Stdio> {
        convert_to_blocking_file(self.inner).map(Stdio::from)
    }
}

impl AsRawFd for ChildStdin {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl AsRawFd for ChildStdout {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl AsyncWrite for ChildStdin {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        self.inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        true
    }
}

impl AsyncRead for ChildStdout {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // SAFETY: Receiver reads into the buffer without inspecting uninitialized bytes.
        unsafe { self.inner.poll_read(cx, buf) }
    }
}

fn set_nonblocking<T: AsRawFd>(fd: &mut T, nonblocking: bool) -> io::Result<()> {
    unsafe {
        let fd = fd.as_raw_fd();
        let previous = libc::fcntl(fd, libc::F_GETFL);
        if previous == -1 {
            return Err(io::Error::last_os_error());
        }

        let new = if nonblocking {
            previous | libc::O_NONBLOCK
        } else {
            previous & !libc::O_NONBLOCK
        };

        let r = libc::fcntl(fd, libc::F_SETFL, new);
        if r == -1 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

pub(super) fn stdio<T, U>(io: T) -> io::Result<U>
where
    U: TryFrom<T, Error = io::Error>,
{
    U::try_from(io)
}

#[cfg(all(test, not(loom)))]
mod orphan_on_error_tests {
    use super::orphan_child_on_error;
    use crate::process::unix::orphan::test::MockQueue;
    use crate::process::unix::orphan::Wait;
    use std::io;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    struct MockWait {
        total_waits: usize,
        /// Number of `try_wait` calls that return `Ok(None)` before reporting exit.
        still_running_waits: usize,
    }

    impl MockWait {
        fn still_running() -> Self {
            Self {
                total_waits: 0,
                still_running_waits: 1,
            }
        }

        fn already_exited() -> Self {
            Self {
                total_waits: 0,
                still_running_waits: 0,
            }
        }
    }

    impl Wait for MockWait {
        fn id(&self) -> u32 {
            42
        }

        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            let waits = self.total_waits;
            self.total_waits += 1;
            if waits < self.still_running_waits {
                Ok(None)
            } else {
                Ok(Some(ExitStatus::from_raw(0)))
            }
        }
    }

    #[test]
    fn orphan_child_on_error_enqueues_when_still_running() {
        let queue = MockQueue::new();
        let err = io::Error::from_raw_os_error(libc::EMFILE);
        let returned = orphan_child_on_error(MockWait::still_running(), &queue, err);
        assert_eq!(returned.raw_os_error(), Some(libc::EMFILE));
        assert_eq!(queue.all_enqueued.borrow().len(), 1);
    }

    #[test]
    fn orphan_child_on_error_skips_queue_when_already_exited() {
        let queue = MockQueue::new();
        let err = io::Error::from_raw_os_error(libc::EMFILE);
        let returned = orphan_child_on_error(MockWait::already_exited(), &queue, err);
        assert_eq!(returned.raw_os_error(), Some(libc::EMFILE));
        assert!(queue.all_enqueued.borrow().is_empty());
    }

    #[cfg(all(feature = "rt", not(miri)))]
    struct KillChild {
        pid: libc::pid_t,
    }

    #[cfg(all(feature = "rt", not(miri)))]
    impl Drop for KillChild {
        fn drop(&mut self) {
            if self.pid <= 0 {
                return;
            }
            let pid = self.pid;
            self.pid = 0;
            // SAFETY: `pid` is the child this test spawned. `build_child` moved
            // the `Child` onto the orphan queue, so this guard only has the pid.
            // `SIGKILL` cannot be caught. A null status pointer is valid for
            // `waitpid`. `EINTR` retries; any other error means the child is
            // already gone, including when this runs during an assertion unwind.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                loop {
                    let rc = libc::waitpid(pid, std::ptr::null_mut(), 0);
                    if rc >= 0 {
                        break;
                    }
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() == Some(libc::EINTR) {
                        continue;
                    }
                    break;
                }
            }
        }
    }

    #[cfg(all(feature = "rt", not(miri)))]
    fn pipe_ends() -> (std::os::unix::io::OwnedFd, std::os::unix::io::OwnedFd) {
        use std::os::unix::io::FromRawFd;

        let mut fds = [0; 2];
        // SAFETY: `pipe` writes two open descriptors into `fds` on success.
        let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(rc, 0, "pipe: {}", io::Error::last_os_error());
        for fd in fds {
            // SAFETY: `fd` is one of the descriptors `pipe` just returned.
            // `ChildStdout::from` requires `FD_CLOEXEC`.
            let set = unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
            assert_eq!(set, 0, "FD_CLOEXEC: {}", io::Error::last_os_error());
        }
        // SAFETY: both descriptors are open. This function takes ownership.
        unsafe {
            (
                std::os::unix::io::OwnedFd::from_raw_fd(fds[0]),
                std::os::unix::io::OwnedFd::from_raw_fd(fds[1]),
            )
        }
    }

    #[cfg(all(feature = "rt", not(miri)))]
    fn assert_read_end_closed(write: &std::os::unix::io::OwnedFd) {
        use std::os::unix::io::AsRawFd;

        let buf = [0u8; 1];
        // SAFETY: `write` is still open, and the buffer is one byte.
        let rc = unsafe { libc::write(write.as_raw_fd(), buf.as_ptr().cast(), 1) };
        assert_eq!(rc, -1, "parent read end is still open");
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(libc::EPIPE),
            "parent read end was not closed"
        );
    }

    /// `orphan_child_on_error` never sees the stdio pipes, so those tests stay
    /// green if a failed registration leaves the other parent ends open.
    #[cfg(all(feature = "rt", not(miri)))]
    #[test]
    fn build_child_closes_remaining_pipes_when_stdio_registration_fails() {
        let rt = crate::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("runtime");
        let handle = rt.handle().clone();
        // `add_source` fails once the driver is shut down, on every OS.
        drop(rt);

        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn sleep");
        // Pipes are created after spawn so `sleep` does not inherit the read
        // ends. Other tests in this process reuse descriptor numbers, so
        // `F_GETFD` after close is not proof. `EPIPE` on the write end means
        // that read end was dropped, which unblocks a child blocked on a full
        // pipe. stdin is closed inside the failed `stdio` call either way.
        let (stdout_read, stdout_write) = pipe_ends();
        let (stderr_read, stderr_write) = pipe_ends();
        child.stdout = Some(std::process::ChildStdout::from(stdout_read));
        child.stderr = Some(std::process::ChildStderr::from(stderr_read));
        let _kill = KillChild {
            pid: libc::pid_t::try_from(child.id()).expect("pid"),
        };
        let _enter = handle.enter();

        let err = match super::build_child(child) {
            Ok(_spawned) => panic!("stdio registration should fail"),
            Err(err) => err,
        };
        assert!(
            crate::runtime::is_rt_shutdown_err(&err),
            "expected the shut-down runtime error, got {err:?}"
        );
        assert_read_end_closed(&stdout_write);
        assert_read_end_closed(&stderr_write);
    }
}
