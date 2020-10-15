use std::io;
use std::process::ExitStatus;
use std::sync::Mutex;

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

/// An interface for reaping a set of orphaned processes.
pub(crate) trait ReapOrphanQueue {
    /// Attempts to reap every process in the queue, ignoring any errors and
    /// enqueueing any orphans which have not yet exited.
    fn reap_orphans(&self);
}

impl<T: ReapOrphanQueue> ReapOrphanQueue for &T {
    fn reap_orphans(&self) {
        (**self).reap_orphans()
    }
}

/// An interface for queueing up an orphaned process so that it can be reaped.
pub(crate) trait OrphanQueue<T>: ReapOrphanQueue {
    /// Adds an orphan to the queue.
    fn push_orphan(&self, orphan: T);
}

impl<T, O: OrphanQueue<T>> OrphanQueue<T> for &O {
    fn push_orphan(&self, orphan: T) {
        (**self).push_orphan(orphan);
    }
}

/// An implementation of `OrphanQueue`.
#[derive(Debug)]
pub(crate) struct OrphanQueueImpl<T> {
    queue: Mutex<Vec<T>>,
}

impl<T> OrphanQueueImpl<T> {
    pub(crate) fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}

impl<T: Wait> OrphanQueue<T> for OrphanQueueImpl<T> {
    fn push_orphan(&self, orphan: T) {
        self.queue.lock().unwrap().push(orphan)
    }
}

impl<T: Wait> ReapOrphanQueue for OrphanQueueImpl<T> {
    fn reap_orphans(&self) {
        let mut queue = self.queue.lock().unwrap();
        let queue = &mut *queue;

        for i in (0..queue.len()).rev() {
            match queue[i].try_wait() {
                Ok(None) => {}
                Ok(Some(_)) | Err(_) => {
                    // The stdlib handles interruption errors (EINTR) when polling a child process.
                    // All other errors represent invalid inputs or pids that have already been
                    // reaped, so we can drop the orphan in case an error is raised.
                    queue.swap_remove(i);
                }
            }
        }
    }
}

#[cfg(all(test, not(loom)))]
pub(crate) mod test {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::io;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use std::rc::Rc;

    pub(crate) struct MockQueue<W> {
        pub(crate) all_enqueued: RefCell<Vec<W>>,
        pub(crate) total_reaps: Cell<usize>,
    }

    impl<W> MockQueue<W> {
        pub(crate) fn new() -> Self {
            Self {
                all_enqueued: RefCell::new(Vec::new()),
                total_reaps: Cell::new(0),
            }
        }
    }

    impl<W> OrphanQueue<W> for MockQueue<W> {
        fn push_orphan(&self, orphan: W) {
            self.all_enqueued.borrow_mut().push(orphan);
        }
    }

    impl<W> ReapOrphanQueue for MockQueue<W> {
        fn reap_orphans(&self) {
            self.total_reaps.set(self.total_reaps.get() + 1);
        }
    }

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

        let orphanage = OrphanQueueImpl::new();
        orphanage.push_orphan(first_orphan);
        orphanage.push_orphan(third_orphan);
        orphanage.push_orphan(second_orphan);
        orphanage.push_orphan(fourth_orphan);

        assert_eq!(orphanage.len(), 4);

        orphanage.reap_orphans();
        assert_eq!(orphanage.len(), 2);
        assert_eq!(first_waits.get(), 1);
        assert_eq!(second_waits.get(), 1);
        assert_eq!(third_waits.get(), 1);
        assert_eq!(fourth_waits.get(), 1);

        orphanage.reap_orphans();
        assert_eq!(orphanage.len(), 1);
        assert_eq!(first_waits.get(), 1);
        assert_eq!(second_waits.get(), 2);
        assert_eq!(third_waits.get(), 2);
        assert_eq!(fourth_waits.get(), 1);

        orphanage.reap_orphans();
        assert_eq!(orphanage.len(), 0);
        assert_eq!(first_waits.get(), 1);
        assert_eq!(second_waits.get(), 2);
        assert_eq!(third_waits.get(), 3);
        assert_eq!(fourth_waits.get(), 1);

        orphanage.reap_orphans(); // Safe to reap when empty
    }
}
