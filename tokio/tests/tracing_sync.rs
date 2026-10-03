//! Tests for sync instrumentation.
//!
//! These tests ensure that the instrumentation for tokio
//! synchronization primitives is correct.
#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, feature = "tracing", target_has_atomic = "64"))]

use tokio::sync;
use tracing_mock::{expect, subscriber};

#[tokio::test]
async fn test_barrier_creates_span() {
    let barrier_span = expect::span()
        .named("runtime.resource")
        .with_target("tokio::sync::barrier");

    let size_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("size").with_value(&1_u64));

    let arrived_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("arrived").with_value(&0_i64));

    let (subscriber, handle) = subscriber::mock()
        .new_span(
            barrier_span
                .clone()
                .with_ancestry(expect::is_explicit_root()),
        )
        .enter(&barrier_span)
        .event(size_event)
        .event(arrived_event)
        .exit(&barrier_span)
        .drop_span(&barrier_span)
        .run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        let _ = sync::Barrier::new(1);
    }

    handle.assert_finished();
}

#[tokio::test]
async fn test_mutex_creates_span() {
    let mutex_span = expect::span()
        .named("runtime.resource")
        .with_target("tokio::sync::mutex");

    let locked_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("locked").with_value(&false));

    let batch_semaphore_span = expect::span()
        .named("runtime.resource")
        .with_target("tokio::sync::batch_semaphore");

    let batch_semaphore_permits_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("permits").with_value(&1u64))
        .with_fields(expect::field("permits.op").with_value(&"override"));

    let (subscriber, handle) = subscriber::mock()
        .new_span(mutex_span.clone().with_ancestry(expect::is_explicit_root()))
        .enter(&mutex_span)
        .event(locked_event)
        .new_span(
            batch_semaphore_span
                .clone()
                .with_ancestry(expect::is_explicit_root()),
        )
        .enter(&batch_semaphore_span)
        .event(batch_semaphore_permits_event)
        .exit(&batch_semaphore_span)
        .exit(&mutex_span)
        .drop_span(&mutex_span)
        .drop_span(&batch_semaphore_span)
        .run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        let _ = sync::Mutex::new(true);
    }

    handle.assert_finished();
}

#[tokio::test]
async fn test_oneshot_creates_span() {
    let oneshot_span_id = expect::id();
    let oneshot_span = expect::span()
        .with_id(oneshot_span_id.clone())
        .named("runtime.resource")
        .with_target("tokio::sync::oneshot");

    let initial_tx_dropped_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("tx_dropped").with_value(&false))
        .with_fields(expect::field("tx_dropped.op").with_value(&"override"));

    let final_tx_dropped_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("tx_dropped").with_value(&true))
        .with_fields(expect::field("tx_dropped.op").with_value(&"override"));

    let initial_rx_dropped_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("rx_dropped").with_value(&false))
        .with_fields(expect::field("rx_dropped.op").with_value(&"override"));

    let final_rx_dropped_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("rx_dropped").with_value(&true))
        .with_fields(expect::field("rx_dropped.op").with_value(&"override"));

    let value_sent_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("value_sent").with_value(&false))
        .with_fields(expect::field("value_sent.op").with_value(&"override"));

    let value_received_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("value_received").with_value(&false))
        .with_fields(expect::field("value_received.op").with_value(&"override"));

    let async_op_span_id = expect::id();
    let async_op_span = expect::span()
        .with_id(async_op_span_id.clone())
        .named("runtime.resource.async_op")
        .with_target("tokio::sync::oneshot");

    let async_op_poll_span = expect::span()
        .named("runtime.resource.async_op.poll")
        .with_target("tokio::sync::oneshot");

    let (subscriber, handle) = subscriber::mock()
        .new_span(
            oneshot_span
                .clone()
                .with_ancestry(expect::is_explicit_root()),
        )
        .enter(&oneshot_span)
        .event(initial_tx_dropped_event)
        .exit(&oneshot_span)
        .enter(&oneshot_span)
        .event(initial_rx_dropped_event)
        .exit(&oneshot_span)
        .enter(&oneshot_span)
        .event(value_sent_event)
        .exit(&oneshot_span)
        .enter(&oneshot_span)
        .event(value_received_event)
        .exit(&oneshot_span)
        .enter(&oneshot_span)
        .new_span(
            async_op_span
                .clone()
                .with_ancestry(expect::has_contextual_parent(&oneshot_span_id)),
        )
        .exit(&oneshot_span)
        .enter(&async_op_span)
        .new_span(
            async_op_poll_span
                .clone()
                .with_ancestry(expect::has_contextual_parent(&async_op_span_id)),
        )
        .exit(&async_op_span)
        .enter(&oneshot_span)
        .event(final_tx_dropped_event)
        .exit(&oneshot_span)
        .enter(&oneshot_span)
        .event(final_rx_dropped_event)
        .exit(&oneshot_span)
        .drop_span(oneshot_span)
        .drop_span(async_op_span)
        .drop_span(&async_op_poll_span)
        .run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        let _ = sync::oneshot::channel::<bool>();
    }

    handle.assert_finished();
}

#[tokio::test]
async fn test_rwlock_creates_span() {
    let rwlock_span = expect::span()
        .named("runtime.resource")
        .with_target("tokio::sync::rwlock");

    let max_readers_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("max_readers").with_value(&0x1FFFFFFF_u64));

    let write_locked_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("write_locked").with_value(&false));

    let current_readers_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("current_readers").with_value(&0_i64));

    let batch_semaphore_span = expect::span()
        .named("runtime.resource")
        .with_target("tokio::sync::batch_semaphore");

    let batch_semaphore_permits_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("permits").with_value(&1u64))
        .with_fields(expect::field("permits.op").with_value(&"override"));

    let (subscriber, handle) = subscriber::mock()
        .new_span(
            rwlock_span
                .clone()
                .with_ancestry(expect::is_explicit_root()),
        )
        .enter(rwlock_span.clone())
        .event(max_readers_event)
        .event(write_locked_event)
        .event(current_readers_event)
        .exit(rwlock_span.clone())
        .enter(rwlock_span.clone())
        .new_span(batch_semaphore_span.clone())
        .enter(batch_semaphore_span.clone())
        .event(batch_semaphore_permits_event)
        .exit(batch_semaphore_span.clone())
        .exit(rwlock_span.clone())
        .drop_span(rwlock_span)
        .drop_span(batch_semaphore_span)
        .run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        let _ = sync::RwLock::new(true);
    }

    handle.assert_finished();
}

#[tokio::test]
async fn test_semaphore_creates_span() {
    let semaphore_span = expect::span()
        .named("runtime.resource")
        .with_target("tokio::sync::semaphore");

    let batch_semaphore_span = expect::span()
        .named("runtime.resource")
        .with_target("tokio::sync::batch_semaphore");

    let batch_semaphore_permits_event = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(expect::field("permits").with_value(&1u64))
        .with_fields(expect::field("permits.op").with_value(&"override"));

    let (subscriber, handle) = subscriber::mock()
        .new_span(
            semaphore_span
                .clone()
                .with_ancestry(expect::is_explicit_root()),
        )
        .enter(semaphore_span.clone())
        .new_span(batch_semaphore_span.clone())
        .enter(batch_semaphore_span.clone())
        .event(batch_semaphore_permits_event)
        .exit(batch_semaphore_span.clone())
        .exit(semaphore_span.clone())
        .drop_span(semaphore_span)
        .drop_span(batch_semaphore_span)
        .run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        let _ = sync::Semaphore::new(1);
    }

    handle.assert_finished();
}

/// Tests for panics in the tracing subscriber while a synchronization primitive
/// is in the middle of an operation.
#[cfg(panic = "unwind")]
mod subscriber_panic {
    use std::future::Future;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync;
    use tokio_test::{assert_pending, assert_ready_ok, task};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    /// A subscriber that panics on the first event with the given target.
    ///
    /// Later events, such as those emitted while unwinding, are ignored.
    struct PanicOnEvent {
        target: &'static str,
        panicked: AtomicBool,
    }

    impl PanicOnEvent {
        fn new(target: &'static str) -> Self {
            Self {
                target,
                panicked: AtomicBool::new(false),
            }
        }
    }

    impl Subscriber for PanicOnEvent {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            metadata.target() == self.target
        }

        fn event(&self, _event: &Event<'_>) {
            if !self.panicked.swap(true, Ordering::SeqCst) {
                panic!("tracing subscriber panicked");
            }
        }

        fn new_span(&self, _span: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}

        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

        fn enter(&self, _span: &Id) {}

        fn exit(&self, _span: &Id) {}
    }

    #[test]
    fn semaphore_release() {
        let sem = sync::Semaphore::new(0);
        let mut acquire = task::spawn(sem.acquire());
        assert_pending!(acquire.poll());

        // The subscriber panics on the event emitted once the permit has been
        // assigned to the waiter.
        let subscriber = PanicOnEvent::new("runtime::resource::async_op::state_update");
        let res = catch_unwind(AssertUnwindSafe(|| {
            tracing::subscriber::with_default(subscriber, || sem.add_permits(1));
        }));
        assert!(res.is_err());

        // The waiter has all of its permits, so the acquisition completes. It
        // must have been removed from the wait queue, so dropping it and then
        // releasing another permit must not access the freed waiter.
        let permit = assert_ready_ok!(acquire.poll());
        drop(acquire);
        sem.add_permits(1);
        drop(permit);
        assert_eq!(sem.available_permits(), 2);
    }

    #[test]
    fn semaphore_acquire() {
        // Polls `acquire` with a subscriber that panics on the event emitted
        // once the permits have been taken from the semaphore. The `Acquire`
        // future is dropped while unwinding, which must return the permits.
        fn poll_with_panicking_subscriber<F: Future>(mut acquire: task::Spawn<F>) {
            let subscriber = PanicOnEvent::new("runtime::resource::state_update");
            let res = catch_unwind(AssertUnwindSafe(|| {
                tracing::subscriber::with_default(subscriber, || {
                    let _ = acquire.poll();
                });
            }));
            assert!(res.is_err());
        }

        let sem = sync::Semaphore::new(1);

        // Uncontended acquisition that takes the permit without queueing.
        poll_with_panicking_subscriber(task::spawn(sem.acquire()));
        assert_eq!(sem.available_permits(), 1);

        // Contended acquisition that takes the permit and queues the waiter.
        poll_with_panicking_subscriber(task::spawn(sem.acquire_many(2)));
        assert_eq!(sem.available_permits(), 1);
    }
}
