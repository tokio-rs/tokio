use futures::{Async, Future, Poll, Stream};
use std::io;
use std::ops::Deref;
use std::process::ExitStatus;

/// An interface for waiting on a process to exit.
pub trait Wait {
    /// Try waiting for a process to exit in a non-blocking manner.
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>>;
}

/// An interface for killing a running process.
pub trait Kill {
    /// Forcefully kill the process.
    fn kill(&mut self) -> io::Result<()>;
}

#[derive(Debug, PartialEq)]
enum WaitResult {
    Exited(ExitStatus),
    Reaped,
}

/// An interface for safely reaping a child process.
trait Reap {
    /// Try to reap the child process if ready.
    fn try_reap(&mut self) -> Poll<WaitResult, io::Error>;
}

#[derive(Debug)]
struct Reaper<W> {
    reaped: bool,
    proc: W,
}

impl<W> Reaper<W> {
    fn new(proc: W) -> Self {
        Self {
            reaped: false,
            proc,
        }
    }

    fn reaped(&self) -> bool {
        self.reaped
    }
}

impl<W> Deref for Reaper<W> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.proc
    }
}

impl<W: Wait> Reap for Reaper<W> {
    fn try_reap(&mut self) -> Poll<WaitResult, io::Error> {
        if self.reaped {
            return Ok(Async::Ready(WaitResult::Reaped));
        }

        match self.proc.try_wait()? {
            Some(exit) => {
                self.reaped = true;
                Ok(Async::Ready(WaitResult::Exited(exit)))
            },
            None => Ok(Async::NotReady),
        }
    }
}

impl<W: Kill> Kill for Reaper<W> {
    fn kill(&mut self) -> io::Result<()> {
        // NB: ensure we don't issue a kill after we've reaped the child
        // since its process identifier could have been reused.
        if self.reaped {
            Ok(())
        } else {
            self.proc.kill()
        }
    }
}

/// Orchestrates between registering interest for receiving signals when a
/// child process has exited, and attempting to poll for process completion.
#[derive(Debug)]
pub struct EventedReaper<W, S> {
    inner: Reaper<W>,
    signal: S,
}

impl<W, S> Deref for EventedReaper<W, S> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl<W, S> EventedReaper<W, S> {
    pub fn new(inner: W, signal: S) -> Self {
        Self {
            inner: Reaper::new(inner),
            signal,
        }
    }
}

impl<W, S> Future for EventedReaper<W, S>
    where W: Wait,
          S: Stream<Error = io::Error>,
{
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            // Ensure we don't register for additional notifications
            // if the child has already finished.
            if self.inner.reaped() {
                return Ok(Async::NotReady);
            }

            // If the child hasn't exited yet, then it's our responsibility to
            // ensure the current task gets notified when it might be able to
            // make progress.
            //
            // As described in `spawn` above, we just indicate that we can
            // next make progress once a SIGCHLD is received.
            //
            // However, we will register for a notification on the next signal
            // BEFORE we poll the child. Otherwise it is possible that the child
            // can exit and the signal can arrive after we last polled the child,
            // but before we've registered for a notification on the next signal
            // (this can cause a deadlock if there are no more spawned children
            // which can generate a different signal for us). A side effect of
            // pre-registering for signal notifications is that when the child
            // exits, we will have already registered for an additional
            // notification we don't need to consume. If another signal arrives,
            // this future's task will be notified/woken up again. Since the
            // futures model allows for spurious wake ups this extra wakeup
            // should not cause significant issues with parent futures.
            let registered_interest = self.signal.poll()?.is_not_ready();

            if let Async::Ready(WaitResult::Exited(status)) = self.inner.try_reap()? {
                return Ok(Async::Ready(status));
            }

            // If our attempt to poll for the next signal was not ready, then
            // we've arranged for our task to get notified and we can bail out.
            if registered_interest {
                return Ok(Async::NotReady);
            } else {
                // Otherwise, if the signal stream delivered a signal to us, we
                // won't get notified at the next signal, so we'll loop and try
                // again.
                continue;
            }
        }
    }
}

impl<W, S> Kill for EventedReaper<W, S>
    where W: Kill,
{
    fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }
}

#[cfg(test)]
mod test {
    use futures::{Async, Poll, Stream};
    use std::process::ExitStatus;
    use std::os::unix::process::ExitStatusExt;
    use super::*;

    struct MockWait {
        total_kills: usize,
        total_waits: usize,
        num_wait_until_status: usize,
        status: ExitStatus,
    }

    impl MockWait {
        fn new(status: ExitStatus, num_wait_until_status: usize) -> Self {
            Self {
                total_kills: 0,
                total_waits: 0,
                num_wait_until_status,
                status
            }
        }
    }

    impl Wait for MockWait {
        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            let ret = if self.num_wait_until_status == self.total_waits {
                Some(self.status.clone())
            } else {
                None
            };

            self.total_waits += 1;
            Ok(ret)
        }
    }

    impl Kill for MockWait {
        fn kill(&mut self) -> io::Result<()> {
            self.total_kills += 1;
            Ok(())
        }
    }

    struct MockStream {
        total_polls: usize,
        values: Vec<Option<()>>,
    }

    impl MockStream {
        fn new(values: Vec<Option<()>>) -> Self {
            Self {
                total_polls: 0,
                values
            }
        }
    }

    impl Stream for MockStream {
        type Item = ();
        type Error = io::Error;

        fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
            self.total_polls += 1;
            match self.values.remove(0) {
                Some(()) => Ok(Async::Ready(Some(()))),
                None => Ok(Async::NotReady),
            }
        }
    }

    #[test]
    fn reaper() {
        let exit = ExitStatus::from_raw(0);
        let mock = MockWait::new(exit.clone(), 1);
        let mut grim = Reaper::new(mock);

        // Not yet exited
        assert_eq!(Async::NotReady, grim.try_reap().expect("failed to wait"));
        assert_eq!(1, grim.total_waits);

        // Exited
        assert_eq!(Async::Ready(WaitResult::Exited(exit)), grim.try_reap().expect("failed to wait"));
        assert_eq!(2, grim.total_waits);

        // Cannot call wait another time
        assert_eq!(Async::Ready(WaitResult::Reaped), grim.try_reap().expect("failed to wait"));
        assert_eq!(2, grim.total_waits);
    }

    #[test]
    fn evented_reaper() {
        let exit = ExitStatus::from_raw(0);
        let mock = MockWait::new(exit.clone(), 3);
        let mut grim = EventedReaper::new(mock, MockStream::new(vec!(
            None,
            Some(()),
            None,
            None,
            None,
        )));

        // Not yet exited, interest registered
        assert_eq!(Async::NotReady, grim.poll().expect("failed to wait"));
        assert_eq!(1, grim.signal.total_polls);
        assert_eq!(1, grim.total_waits);

        // Not yet exited, couldn't register interest the first time
        // but managed to register interest the second time around
        assert_eq!(Async::NotReady, grim.poll().expect("failed to wait"));
        assert_eq!(3, grim.signal.total_polls);
        assert_eq!(3, grim.total_waits);

        // Exited
        assert_eq!(Async::Ready(exit), grim.poll().expect("failed to wait"));
        assert_eq!(4, grim.signal.total_polls);
        assert_eq!(4, grim.total_waits);

        // Already reaped, no further calls
        assert_eq!(Async::NotReady, grim.poll().expect("failed to poll"));
        assert_eq!(4, grim.signal.total_polls);
        assert_eq!(4, grim.total_waits);
    }

    #[test]
    fn kill() {
        let exit = ExitStatus::from_raw(0);
        let mut grim = EventedReaper::new(
            MockWait::new(exit, 0),
            MockStream::new(vec!(None))
        );

        grim.kill().unwrap();
        assert_eq!(1, grim.total_kills);

        // Do not kill after reaping
        assert_eq!(Async::Ready(exit), grim.poll().expect("failed to poll"));
        grim.kill().unwrap();
        assert_eq!(1, grim.total_kills);
    }
}
