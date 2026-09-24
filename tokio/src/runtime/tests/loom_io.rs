use crate::io::ready::Ready;
use crate::io::Interest;
use crate::loom::sync::Arc;
use crate::loom::thread;
use crate::runtime::io::{Direction, Handle};
use loom::future::block_on;
use std::task::Poll;

/// Tests that when an I/O source is deregistered and its user-facing `Arc<ScheduledIo>`
/// is dropped, an already-obtained event delivered via the raw pointer `Token` does
/// not cause a use-after-free or data race because release is deferred until before the next poll.
#[test]
fn deregister_event_delivery_race_lifetime() {
    loom::model(|| {
        let handle = Arc::new(Handle::new_mock());
        let scheduled_io = handle.allocate().unwrap();
        let token = scheduled_io.token();

        let handle_clone = handle.clone();
        let driver_th = thread::spawn(move || {
            // Driver thread: has already obtained an event from poll() for `token`.
            // Delivers event to ScheduledIo via its exposed raw pointer token.
            unsafe {
                handle_clone.dispatch_event(token, Ready::READABLE);
            }
            // Simulates deferred release performed at the start of the next turn/poll.
            handle_clone.release_pending_registrations();
        });

        // Worker thread: deregisters the resource and drops its Arc handle.
        handle.deregister_io(&scheduled_io);
        drop(scheduled_io);

        driver_th.join().unwrap();

        // Flush any remaining release if worker deregistered after driver's release step.
        handle.release_pending_registrations();
    });
}

/// Tests readiness monotonicity and consistency under concurrent event delivery and deregistration.
#[test]
fn deregister_and_readiness_interleaving() {
    loom::model(|| {
        let handle = Arc::new(Handle::new_mock());
        let scheduled_io = handle.allocate().unwrap();
        let token = scheduled_io.token();

        let handle_clone = handle.clone();
        let driver_th = thread::spawn(move || {
            unsafe {
                handle_clone.dispatch_event(token, Ready::READABLE);
            }
            handle_clone.release_pending_registrations();
        });

        // Worker inspects ready event, deregisters, and drops.
        let ev_before = scheduled_io.ready_event(Interest::READABLE);
        handle.deregister_io(&scheduled_io);
        let ev_after = scheduled_io.ready_event(Interest::READABLE);
        drop(scheduled_io);

        driver_th.join().unwrap();
        handle.release_pending_registrations();

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
        let handle = Arc::new(Handle::new_mock());
        let scheduled_io = handle.allocate().unwrap();
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
            handle.dispatch_event(token, Ready::READABLE);
        }

        waiter_th.join().unwrap();

        handle.deregister_io(&scheduled_io);
        drop(scheduled_io);
        handle.release_pending_registrations();
    });
}

/// Sequential invariant: dropping the user's Arc after deregistration preserves
/// the ScheduledIo in pending_release so that a delayed dispatch_event remains safe
/// until release_pending_registrations is explicitly called.
#[test]
fn deferred_release_keeps_scheduled_io_alive_after_deregister() {
    loom::model(|| {
        let handle = Handle::new_mock();
        let scheduled_io = handle.allocate().unwrap();
        let token = scheduled_io.token();

        // Deregister and drop user handle.
        handle.deregister_io(&scheduled_io);
        drop(scheduled_io);

        // Raw-pointer dispatch must still succeed without UAF.
        unsafe {
            handle.dispatch_event(token, Ready::READABLE);
        }

        // Release on next turn frees the memory.
        handle.release_pending_registrations();
    });
}
