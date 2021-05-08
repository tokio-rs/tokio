#![cfg_attr(not(feature = "rt"), allow(dead_code))]

//! Process driver

use crate::park::Park;
use crate::process::unix::orphan::ReapOrphanQueue;
use crate::process::unix::GlobalOrphanQueue;
use crate::signal::unix::driver::Driver as SignalDriver;
use crate::signal::unix::{signal_with_handle, SignalKind};
use crate::sync::watch;

use std::io;
use std::time::Duration;

/// Responsible for cleaning up orphaned child processes on Unix platforms.
#[derive(Debug)]
pub(crate) struct Driver {
    park: SignalDriver,
    inner: CoreDriver<watch::Receiver<()>, GlobalOrphanQueue>,
}

#[derive(Debug)]
struct CoreDriver<S, Q> {
    sigchild: S,
    orphan_queue: Q,
}

trait HasChanged {
    fn has_changed(&mut self) -> bool;
}

impl<T> HasChanged for watch::Receiver<T> {
    fn has_changed(&mut self) -> bool {
        self.try_has_changed().and_then(Result::ok).is_some()
    }
}

// ===== impl CoreDriver =====

impl<S, Q> CoreDriver<S, Q>
where
    S: HasChanged,
    Q: ReapOrphanQueue,
{
    fn process(&mut self) {
        if self.sigchild.has_changed() {
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

    impl HasChanged for MockStream {
        fn has_changed(&mut self) -> bool {
            self.total_try_recv += 1;
            self.values.remove(0).is_some()
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
}
