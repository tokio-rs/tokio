use crate::io::ready::Ready;
use crate::io::Interest;
use crate::loom::sync::Arc;
use crate::loom::sync::Mutex;
use crate::loom::thread;
use crate::runtime::io::{dispatch_event, Direction, RegistrationSet, ScheduledIo, Synced};
use loom::future::block_on;
use std::task::Poll;

// Models only Tokio's registration lifetime. Mio polling and OS deregistration
// are outside the model; the production Handle remains unchanged under Loom.
struct Registrations {
    set: RegistrationSet,
    synced: Mutex<Synced>,
}

impl Registrations {
    fn new() -> Self {
        let (set, synced) = RegistrationSet::new();
        Self {
            set,
            synced: Mutex::new(synced),
        }
    }

    fn allocate(&self) -> Arc<ScheduledIo> {
        self.set.allocate(&mut self.synced.lock()).unwrap()
    }

    fn deregister(&self, io: &Arc<ScheduledIo>) {
        self.set.deregister(&mut self.synced.lock(), io);
    }

    fn release(&self) {
        if self.set.needs_release() {
            self.set.release(&mut self.synced.lock());
        }
    }
}

/// Models deregistration racing with delivery of an event already returned by
/// the poller. The driver completes delivery before its next release step.
#[test]
fn deregister_races_with_prior_event_delivery() {
    loom::model(|| {
        let handle = Arc::new(Registrations::new());
        let scheduled_io = handle.allocate();
        let token = scheduled_io.token();

        let handle_clone = handle.clone();
        let driver_th = thread::spawn(move || {
            // Driver thread: has already obtained an event from poll() for `token`.
            // Delivers event to ScheduledIo via its exposed raw pointer token.
            unsafe {
                dispatch_event(token, Ready::READABLE);
            }
            // Simulates deferred release performed at the start of the next turn/poll.
            handle_clone.release();
        });

        // Worker thread: deregisters the resource and drops its Arc handle.
        handle.deregister(&scheduled_io);
        drop(scheduled_io);

        driver_th.join().unwrap();

        // Flush any remaining release if worker deregistered after driver's release step.
        handle.release();
    });
}

/// Tests that readiness already observed by a task is not lost during concurrent event delivery and deregistration.
#[test]
fn deregister_and_readiness_interleaving() {
    loom::model(|| {
        let handle = Arc::new(Registrations::new());
        let scheduled_io = handle.allocate();
        let token = scheduled_io.token();

        let handle_clone = handle.clone();
        let driver_th = thread::spawn(move || {
            unsafe {
                dispatch_event(token, Ready::READABLE);
            }
            handle_clone.release();
        });

        // Worker inspects ready event, deregisters, and drops.
        let ev_before = scheduled_io.ready_event(Interest::READABLE);
        handle.deregister(&scheduled_io);
        let ev_after = scheduled_io.ready_event(Interest::READABLE);
        drop(scheduled_io);

        driver_th.join().unwrap();
        handle.release();

        // Monotonicity: if readiness was observed before deregistration, it must remain ready after.
        if ev_before.ready.is_readable() {
            assert!(ev_after.ready.is_readable());
        }
    });
}

/// Tests that delivering an event via raw pointer correctly wakes a pending task waiting
/// on readiness via `poll_readiness`.
#[test]
fn event_delivery_wakes_waiter() {
    loom::model(|| {
        let handle = Arc::new(Registrations::new());
        let scheduled_io = handle.allocate();
        let token = scheduled_io.token();

        let io_clone = scheduled_io.clone();
        let waiter_th = thread::spawn(move || {
            let ev = block_on(std::future::poll_fn(|cx| {
                match io_clone.poll_readiness(cx, Direction::Read) {
                    Poll::Ready(ev) => Poll::Ready(ev),
                    Poll::Pending => Poll::Pending,
                }
            }));
            assert!(ev.ready.is_readable());
        });

        // Driver delivers event and wakes the waiter.
        unsafe {
            dispatch_event(token, Ready::READABLE);
        }

        waiter_th.join().unwrap();

        handle.deregister(&scheduled_io);
        drop(scheduled_io);
        handle.release();
    });
}

/// Models delivery after deregistration and dropping the caller's Arc, but
/// before the driver's next release step. This does not model Mio's guarantee
/// about whether a deregistered source can produce a new event.
#[test]
fn deferred_release_allows_prior_event_delivery() {
    loom::model(|| {
        let handle = Registrations::new();
        let scheduled_io = handle.allocate();
        let token = scheduled_io.token();

        // Deregister and drop user handle.
        handle.deregister(&scheduled_io);
        drop(scheduled_io);

        // Delivery precedes the next release step in this model.
        unsafe {
            dispatch_event(token, Ready::READABLE);
        }

        // Release on next turn frees the memory.
        handle.release();
    });
}
