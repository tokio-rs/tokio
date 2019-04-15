#[macro_use]
extern crate tokio_trace;
use tokio_trace::{
    span,
    subscriber::{with_default, Interest, Subscriber},
    Event, Level, Metadata,
};

#[test]
fn event_macros_dont_infinite_loop() {
    // This test ensures that an event macro within a subscriber
    // won't cause an infinite loop of events.
    struct TestSubscriber;
    impl Subscriber for TestSubscriber {
        fn register_callsite(&self, _: &Metadata) -> Interest {
            // Always return sometimes so that `enabled` will be called
            // (which can loop).
            Interest::sometimes()
        }

        fn enabled(&self, meta: &Metadata) -> bool {
            assert!(meta.fields().iter().any(|f| f.name() == "foo"));
            event!(Level::TRACE, bar = false);
            true
        }

        fn new_span(&self, _: &span::Attributes) -> span::Id {
            span::Id::from_u64(0xAAAA)
        }

        fn record(&self, _: &span::Id, _: &span::Record) {}

        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

        fn event(&self, event: &Event) {
            assert!(event.metadata().fields().iter().any(|f| f.name() == "foo"));
            event!(Level::TRACE, baz = false);
        }

        fn enter(&self, _: &span::Id) {}

        fn exit(&self, _: &span::Id) {}
    }

    with_default(TestSubscriber, || {
        event!(Level::TRACE, foo = false);
    })
}
