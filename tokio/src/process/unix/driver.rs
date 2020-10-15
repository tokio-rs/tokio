#![cfg_attr(not(feature = "rt"), allow(dead_code))]

//! Process driver

use crate::park::Park;
use crate::process::unix::orphan::ReapOrphanQueue;
use crate::process::unix::GlobalOrphanQueue;
use crate::signal::unix::driver::Driver as SignalDriver;
use crate::signal::unix::{signal_with_handle, InternalStream, Signal, SignalKind};
use crate::sync::mpsc::error::TryRecvError;

use std::io;
use std::time::Duration;

/// Responsible for cleaning up orphaned child processes on Unix platforms.
#[derive(Debug)]
pub(crate) struct Driver {
    park: SignalDriver,
    inner: CoreDriver<Signal, GlobalOrphanQueue>,
}

#[derive(Debug)]
struct CoreDriver<S, Q> {
    sigchild: S,
    orphan_queue: Q,
}

// ===== impl CoreDriver =====

impl<S, Q> CoreDriver<S, Q>
where
    S: InternalStream,
    Q: ReapOrphanQueue,
{
    fn got_signal(&mut self) -> bool {
        match self.sigchild.try_recv() {
            Ok(()) => true,
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Closed) => panic!("signal was deregistered"),
        }
    }

    fn process(&mut self) {
        if self.got_signal() {
            // Drain all notifications which may have been buffered
            // so we can try to reap all orphans in one batch
            while self.got_signal() {}

            self.orphan_queue.reap_orphans();
        }
    }
}

// ===== impl Driver =====

impl Driver {
    /// Creates a new signal `Driver` instance that delegates wakeups to `park`.
    pub(crate) fn new(park: SignalDriver) -> io::Result<Self> {
        let sigchild = signal_with_handle(SignalKind::child(), park.handle())?;
        let inner = CoreDriver {
            sigchild,
            orphan_queue: GlobalOrphanQueue,
        };

        Ok(Self { park, inner })
    }
}

// ===== impl Park for Driver =====

impl Park for Driver {
    type Unpark = <SignalDriver as Park>::Unpark;
    type Error = io::Error;

    fn unpark(&self) -> Self::Unpark {
        self.park.unpark()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.park.park()?;
        self.inner.process();
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.park.park_timeout(duration)?;
        self.inner.process();
        Ok(())
    }

    fn shutdown(&mut self) {
        self.park.shutdown()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::process::unix::orphan::test::MockQueue;
    use crate::sync::mpsc::error::TryRecvError;
    use std::task::{Context, Poll};

    struct MockStream {
        total_try_recv: usize,
        values: Vec<Option<()>>,
    }

    impl MockStream {
        fn new(values: Vec<Option<()>>) -> Self {
            Self {
                total_try_recv: 0,
                values,
            }
        }
    }

    impl InternalStream for MockStream {
        fn poll_recv(&mut self, _cx: &mut Context<'_>) -> Poll<Option<()>> {
            unimplemented!();
        }

        fn try_recv(&mut self) -> Result<(), TryRecvError> {
            self.total_try_recv += 1;
            match self.values.remove(0) {
                Some(()) => Ok(()),
                None => Err(TryRecvError::Empty),
            }
        }
    }

    #[test]
    fn no_reap_if_no_signal() {
        let mut driver = CoreDriver {
            sigchild: MockStream::new(vec![None]),
            orphan_queue: MockQueue::<()>::new(),
        };

        driver.process();

        assert_eq!(1, driver.sigchild.total_try_recv);
        assert_eq!(0, driver.orphan_queue.total_reaps.get());
    }

    #[test]
    fn coalesce_signals_before_reaping() {
        let mut driver = CoreDriver {
            sigchild: MockStream::new(vec![Some(()), Some(()), None]),
            orphan_queue: MockQueue::<()>::new(),
        };

        driver.process();

        assert_eq!(3, driver.sigchild.total_try_recv);
        assert_eq!(1, driver.orphan_queue.total_reaps.get());
    }
}
