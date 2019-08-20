use crossbeam_queue::SegQueue;
use std::io;
use std::process::ExitStatus;

/// An interface for waiting on a process to exit.
pub(crate) trait Wait {
    /// Get the identifier for this process or diagnostics.
    fn id(&self) -> u32;
    /// Try waiting for a process to exit in a non-blocking manner.
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>>;
}

impl<T: Wait> Wait for &mut T {
    fn id(&self) -> u32 {
        (**self).id()
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        (**self).try_wait()
    }
}

/// An interface for queueing up an orphaned process so that it can be reaped.
pub(crate) trait OrphanQueue<T> {
    /// Add an orphan to the queue.
    fn push_orphan(&self, orphan: T);
    /// Attempt to reap every process in the queue, ignoring any errors and
    /// enqueueing any orphans which have not yet exited.
    fn reap_orphans(&self);
}

impl<T, O: OrphanQueue<T>> OrphanQueue<T> for &O {
    fn push_orphan(&self, orphan: T) {
        (**self).push_orphan(orphan);
    }

    fn reap_orphans(&self) {
        (**self).reap_orphans()
    }
}

/// An atomic implementation of `OrphanQueue`.
#[derive(Debug)]
pub(crate) struct AtomicOrphanQueue<T> {
    queue: SegQueue<T>,
}

impl<T> AtomicOrphanQueue<T> {
    pub(crate) fn new() -> Self {
        Self {
            queue: SegQueue::new(),
        }
    }
}

impl<T: Wait> OrphanQueue<T> for AtomicOrphanQueue<T> {
    fn push_orphan(&self, orphan: T) {
        self.queue.push(orphan)
    }

    fn reap_orphans(&self) {
        let len = self.queue.len();

        if len == 0 {
            return;
        }

        let mut orphans = Vec::with_capacity(len);
        while let Ok(mut orphan) = self.queue.pop() {
            match orphan.try_wait() {
                Ok(Some(_)) => {}
                Err(e) => error!(
                    message = "leaking orphaned process due to try_wait() error",
                    orphan.id =orphan.id(),
                    error = %e,
                ),

                // Still not done yet, we need to put it back in the queue
                // when were done draining it, so that we don't get stuck
                // in an infinite loop here
                Ok(None) => orphans.push(orphan),
            }
        }

        for orphan in orphans {
            self.queue.push(orphan);
        }
    }
}

#[cfg(test)]
mod test {
    use super::Wait;
    use super::{AtomicOrphanQueue, OrphanQueue};
    use std::cell::Cell;
    use std::io;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use std::rc::Rc;

    struct MockWait {
        total_waits: Rc<Cell<usize>>,
        num_wait_until_status: usize,
        return_err: bool,
    }

    impl MockWait {
        fn new(num_wait_until_status: usize) -> Self {
            Self {
                total_waits: Rc::new(Cell::new(0)),
                num_wait_until_status,
                return_err: false,
            }
        }

        fn with_err() -> Self {
            Self {
                total_waits: Rc::new(Cell::new(0)),
                num_wait_until_status: 0,
                return_err: true,
            }
        }
    }

    impl Wait for MockWait {
        fn id(&self) -> u32 {
            42
        }

        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            let waits = self.total_waits.get();

            let ret = if self.num_wait_until_status == waits {
                if self.return_err {
                    Ok(Some(ExitStatus::from_raw(0)))
                } else {
                    Err(io::Error::new(io::ErrorKind::Other, "mock err"))
                }
            } else {
                Ok(None)
            };

            self.total_waits.set(waits + 1);
            ret
        }
    }

    #[test]
    fn drain_attempts_a_single_reap_of_all_queued_orphans() {
        let first_orphan = MockWait::new(0);
        let second_orphan = MockWait::new(1);
        let third_orphan = MockWait::new(2);
        let fourth_orphan = MockWait::with_err();

        let first_waits = first_orphan.total_waits.clone();
        let second_waits = second_orphan.total_waits.clone();
        let third_waits = third_orphan.total_waits.clone();
        let fourth_waits = fourth_orphan.total_waits.clone();

        let orphanage = AtomicOrphanQueue::new();
        orphanage.push_orphan(first_orphan);
        orphanage.push_orphan(third_orphan);
        orphanage.push_orphan(second_orphan);
        orphanage.push_orphan(fourth_orphan);

        assert_eq!(orphanage.queue.len(), 4);

        orphanage.reap_orphans();
        assert_eq!(orphanage.queue.len(), 2);
        assert_eq!(first_waits.get(), 1);
        assert_eq!(second_waits.get(), 1);
        assert_eq!(third_waits.get(), 1);
        assert_eq!(fourth_waits.get(), 1);

        orphanage.reap_orphans();
        assert_eq!(orphanage.queue.len(), 1);
        assert_eq!(first_waits.get(), 1);
        assert_eq!(second_waits.get(), 2);
        assert_eq!(third_waits.get(), 2);
        assert_eq!(fourth_waits.get(), 1);

        orphanage.reap_orphans();
        assert_eq!(orphanage.queue.len(), 0);
        assert_eq!(first_waits.get(), 1);
        assert_eq!(second_waits.get(), 2);
        assert_eq!(third_waits.get(), 3);
        assert_eq!(fourth_waits.get(), 1);

        orphanage.reap_orphans(); // Safe to reap when empty
    }
}
