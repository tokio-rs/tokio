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

/// Orchestrates between registering interest for receiving signals when a
/// child process has exited, and attempting to poll for process completion.
#[derive(Debug)]
pub struct Reaper<W, S> {
    inner: W,
    signal: S,
}

impl<W, S> Deref for Reaper<W, S> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<W, S> Reaper<W, S> {
    pub fn new(inner: W, signal: S) -> Self {
        Self {
            inner,
            signal,
        }
    }
}

impl<W, S> Future for Reaper<W, S>
    where W: Wait,
          S: Stream<Error = io::Error>,
{
    type Item = ExitStatus;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
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

            if let Some(status) = self.inner.try_wait()? {
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

impl<W, S> Kill for Reaper<W, S>
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
                Some(self.status)
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
        let mock = MockWait::new(exit, 3);
        let mut grim = Reaper::new(mock, MockStream::new(vec!(
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
    }

    #[test]
    fn kill() {
        let exit = ExitStatus::from_raw(0);
        let mut grim = Reaper::new(
            MockWait::new(exit, 0),
            MockStream::new(vec!(None))
        );

        grim.kill().unwrap();
        assert_eq!(1, grim.total_kills);
    }
}
