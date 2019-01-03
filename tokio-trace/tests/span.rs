#[macro_use]
extern crate tokio_trace;
mod support;

use self::support::*;
use std::thread;
use tokio_trace::{dispatcher, Dispatch, Span};

#[test]
fn closed_handle_dropped_when_used() {
    // Test that exiting a span only marks it as "done" when no handles
    // that can re-enter the span exist.
    let subscriber = subscriber::mock()
        .enter(span::mock().named("foo"))
        .drop_span(span::mock().named("bar"))
        .enter(span::mock().named("bar"))
        .exit(span::mock().named("bar"))
        .drop_span(span::mock().named("bar"))
        .exit(span::mock().named("foo"))
        .run();

    dispatcher::with_default(Dispatch::new(subscriber), || {
        span!("foo").enter(|| {
            let bar = span!("bar");
            let mut another_bar = bar.clone();
            drop(bar);

            another_bar.close();
            another_bar.enter(|| {});
            // After we exit `another_bar`, it should close and not be
            // re-entered.
            another_bar.enter(|| {});
        });
    });
}

#[test]
fn handles_to_the_same_span_are_equal() {
    // Create a mock subscriber that will return `true` on calls to
    // `Subscriber::enabled`, so that the spans will be constructed. We
    // won't enter any spans in this test, so the subscriber won't actually
    // expect to see any spans.
    dispatcher::with_default(Dispatch::new(subscriber::mock().run()), || {
        let foo1 = span!("foo");
        let foo2 = foo1.clone();
        // Two handles that point to the same span are equal.
        assert_eq!(foo1, foo2);
    });
}

#[test]
fn handles_to_different_spans_are_not_equal() {
    dispatcher::with_default(Dispatch::new(subscriber::mock().run()), || {
        // Even though these spans have the same name and fields, they will have
        // differing metadata, since they were created on different lines.
        let foo1 = span!("foo", bar = 1u64, baz = false);
        let foo2 = span!("foo", bar = 1u64, baz = false);

        assert_ne!(foo1, foo2);
    });
}

#[test]
fn handles_to_different_spans_with_the_same_metadata_are_not_equal() {
    // Every time time this function is called, it will return a _new
    // instance_ of a span with the same metadata, name, and fields.
    fn make_span() -> Span<'static> {
        span!("foo", bar = 1u64, baz = false)
    }

    dispatcher::with_default(Dispatch::new(subscriber::mock().run()), || {
        let foo1 = make_span();
        let foo2 = make_span();

        assert_ne!(foo1, foo2);
        // assert_ne!(foo1.data(), foo2.data());
    });
}

#[test]
fn spans_always_go_to_the_subscriber_that_tagged_them() {
    let subscriber1 = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done();
    let subscriber1 = Dispatch::new(subscriber1.run());
    let subscriber2 = Dispatch::new(subscriber::mock().run());

    let mut foo = dispatcher::with_default(subscriber1, || {
        let mut foo = span!("foo");
        foo.enter(|| {});
        foo
    });
    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    dispatcher::with_default(subscriber2, move || foo.enter(|| {}));
}

#[test]
fn spans_always_go_to_the_subscriber_that_tagged_them_even_across_threads() {
    let subscriber1 = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done();
    let subscriber1 = Dispatch::new(subscriber1.run());
    let mut foo = dispatcher::with_default(subscriber1, || {
        let mut foo = span!("foo");
        foo.enter(|| {});
        foo
    });

    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    thread::spawn(move || {
        dispatcher::with_default(Dispatch::new(subscriber::mock().run()), || {
            foo.enter(|| {});
        })
    })
    .join()
    .unwrap();
}

#[test]
fn dropping_a_span_calls_drop_span() {
    let (subscriber, handle) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    dispatcher::with_default(Dispatch::new(subscriber), || {
        let mut span = span!("foo");
        span.enter(|| {});
        drop(span);
    });

    handle.assert_finished();
}

#[test]
fn span_closes_after_event() {
    let (subscriber, handle) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .event()
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    dispatcher::with_default(Dispatch::new(subscriber), || {
        span!("foo").enter(|| {
            event!(Level::DEBUG, {}, "my event!");
        });
    });

    handle.assert_finished();
}

#[test]
fn new_span_after_event() {
    let (subscriber, handle) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .event()
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .enter(span::mock().named("bar"))
        .exit(span::mock().named("bar"))
        .drop_span(span::mock().named("bar"))
        .done()
        .run_with_handle();
    dispatcher::with_default(Dispatch::new(subscriber), || {
        span!("foo").enter(|| {
            event!(Level::DEBUG, {}, "my event!");
        });
        span!("bar").enter(|| {});
    });

    handle.assert_finished();
}

#[test]
fn event_outside_of_span() {
    let (subscriber, handle) = subscriber::mock()
        .event()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    dispatcher::with_default(Dispatch::new(subscriber), || {
        debug!("my event!");
        span!("foo").enter(|| {});
    });

    handle.assert_finished();
}

#[test]
fn cloning_a_span_calls_clone_span() {
    let (subscriber, handle) = subscriber::mock()
        .clone_span(span::mock().named("foo"))
        .run_with_handle();
    dispatcher::with_default(Dispatch::new(subscriber), || {
        let span = span!("foo");
        let _span2 = span.clone();
    });

    handle.assert_finished();
}

#[test]
fn drop_span_when_exiting_dispatchers_context() {
    let (subscriber, handle) = subscriber::mock()
        .clone_span(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .run_with_handle();
    dispatcher::with_default(Dispatch::new(subscriber), || {
        let span = span!("foo");
        let _span2 = span.clone();
        drop(span);
    });

    handle.assert_finished();
}

#[test]
fn clone_and_drop_span_always_go_to_the_subscriber_that_tagged_the_span() {
    let (subscriber1, handle1) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .clone_span(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .run_with_handle();
    let subscriber1 = Dispatch::new(subscriber1);
    let subscriber2 = Dispatch::new(subscriber::mock().done().run());

    let mut foo = dispatcher::with_default(subscriber1, || {
        let mut foo = span!("foo");
        foo.enter(|| {});
        foo
    });
    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    dispatcher::with_default(subscriber2, move || {
        let foo2 = foo.clone();
        foo.enter(|| {});
        drop(foo);
        drop(foo2);
    });

    handle1.assert_finished();
}

#[test]
fn span_closes_when_exited() {
    let (subscriber, handle) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    dispatcher::with_default(Dispatch::new(subscriber), || {
        let mut foo = span!("foo");
        assert!(!foo.is_closed());

        foo.enter(|| {});
        assert!(!foo.is_closed());

        foo.close();
        assert!(foo.is_closed());

        // Now that `foo` has closed, entering it should do nothing.
        foo.enter(|| {});
        assert!(foo.is_closed());
    });

    handle.assert_finished();
}

#[test]
fn entering_a_closed_span_again_is_a_no_op() {
    let (subscriber, handle) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    dispatcher::with_default(Dispatch::new(subscriber), || {
        let mut foo = span!("foo");
        foo.close();

        foo.enter(|| {
            // When we exit `foo` this time, it will close, and entering it
            // again will do nothing.
        });

        foo.enter(|| {
            // The subscriber expects nothing else to happen after the first
            // exit.
        });
        assert!(foo.is_closed());
    });

    handle.assert_finished();
}
