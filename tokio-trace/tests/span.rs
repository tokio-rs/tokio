#[macro_use]
extern crate tokio_trace;
mod support;

use self::support::*;
use std::thread;
use tokio_trace::{field::display, subscriber::with_default, Level, Span};

#[test]
fn handles_to_the_same_span_are_equal() {
    // Create a mock subscriber that will return `true` on calls to
    // `Subscriber::enabled`, so that the spans will be constructed. We
    // won't enter any spans in this test, so the subscriber won't actually
    // expect to see any spans.
    with_default(subscriber::mock().run(), || {
        let foo1 = span!("foo");
        let foo2 = foo1.clone();
        // Two handles that point to the same span are equal.
        assert_eq!(foo1, foo2);
    });
}

#[test]
fn handles_to_different_spans_are_not_equal() {
    with_default(subscriber::mock().run(), || {
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
    fn make_span() -> Span {
        span!("foo", bar = 1u64, baz = false)
    }

    with_default(subscriber::mock().run(), || {
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
        .done()
        .run();
    let subscriber2 = subscriber::mock().run();

    let mut foo = with_default(subscriber1, || {
        let mut foo = span!("foo");
        foo.enter(|| {});
        foo
    });
    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    with_default(subscriber2, move || foo.enter(|| {}));
}

#[test]
fn spans_always_go_to_the_subscriber_that_tagged_them_even_across_threads() {
    let subscriber1 = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run();
    let mut foo = with_default(subscriber1, || {
        let mut foo = span!("foo");
        foo.enter(|| {});
        foo
    });

    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    thread::spawn(move || {
        with_default(subscriber::mock().run(), || {
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
    with_default(subscriber, || {
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
        .event(event::mock())
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    with_default(subscriber, || {
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
        .event(event::mock())
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .enter(span::mock().named("bar"))
        .exit(span::mock().named("bar"))
        .drop_span(span::mock().named("bar"))
        .done()
        .run_with_handle();
    with_default(subscriber, || {
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
        .event(event::mock())
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    with_default(subscriber, || {
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
    with_default(subscriber, || {
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
    with_default(subscriber, || {
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
    let subscriber2 = subscriber::mock().done().run();

    let mut foo = with_default(subscriber1, || {
        let mut foo = span!("foo");
        foo.enter(|| {});
        foo
    });
    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    with_default(subscriber2, move || {
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
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        let mut foo = span!("foo");

        foo.enter(|| {});

        drop(foo);
    });

    handle.assert_finished();
}

#[test]
fn moved_field() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock().named("foo").with_field(
                field::mock("bar")
                    .with_value(&display("hello from my span"))
                    .only(),
            ),
        )
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        let from = "my span";
        let mut span = span!("foo", bar = display(format!("hello from {}", from)));
        span.enter(|| {});
    });

    handle.assert_finished();
}

#[test]
fn borrowed_field() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock().named("foo").with_field(
                field::mock("bar")
                    .with_value(&display("hello from my span"))
                    .only(),
            ),
        )
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        let from = "my span";
        let mut message = format!("hello from {}", from);
        let mut span = span!("foo", bar = display(&message));
        span.enter(|| {
            message.insert_str(10, " inside");
        });
    });

    handle.assert_finished();
}

#[test]
// If emitting log instrumentation, this gets moved anyway, breaking the test.
#[cfg(not(feature = "log"))]
fn move_field_out_of_struct() {
    use tokio_trace::field::debug;

    #[derive(Debug)]
    struct Position {
        x: f32,
        y: f32,
    }

    let pos = Position {
        x: 3.234,
        y: -1.223,
    };
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock().named("foo").with_field(
                field::mock("x")
                    .with_value(&debug(3.234))
                    .and(field::mock("y").with_value(&debug(-1.223)))
                    .only(),
            ),
        )
        .new_span(
            span::mock()
                .named("bar")
                .with_field(field::mock("position").with_value(&debug(&pos)).only()),
        )
        .run_with_handle();

    with_default(subscriber, || {
        let pos = Position {
            x: 3.234,
            y: -1.223,
        };
        let mut foo = span!("foo", x = debug(pos.x), y = debug(pos.y));
        let mut bar = span!("bar", position = debug(pos));
        foo.enter(|| {});
        bar.enter(|| {});
    });

    handle.assert_finished();
}

#[test]
fn add_field_after_new_span() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock()
                .named("foo")
                .with_field(field::mock("bar").with_value(&5).only()),
        )
        .record(
            span::mock().named("foo"),
            field::mock("baz").with_value(&true).only(),
        )
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        let mut span = span!("foo", bar = 5, baz);
        span.record("baz", &true);
        span.enter(|| {})
    });

    handle.assert_finished();
}

#[test]
fn add_fields_only_after_new_span() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(span::mock().named("foo"))
        .record(
            span::mock().named("foo"),
            field::mock("bar").with_value(&5).only(),
        )
        .record(
            span::mock().named("foo"),
            field::mock("baz").with_value(&true).only(),
        )
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        let mut span = span!("foo", bar, baz);
        span.record("bar", &5);
        span.record("baz", &true);
        span.enter(|| {})
    });

    handle.assert_finished();
}

#[test]
fn new_span_with_target_and_log_level() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock()
                .named("foo")
                .with_target("app_span")
                .at_level(tokio_trace::Level::DEBUG),
        )
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        span!(target: "app_span", level: tokio_trace::Level::DEBUG, "foo");
    });

    handle.assert_finished();
}

#[test]
fn explicit_root_span_is_root() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(span::mock().named("foo").with_explicit_parent(None))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        span!(parent: None, "foo");
    });

    handle.assert_finished();
}

#[test]
fn explicit_root_span_is_root_regardless_of_ctx() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .new_span(span::mock().named("bar").with_explicit_parent(None))
        .exit(span::mock().named("foo"))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        span!("foo").enter(|| {
            span!(parent: None, "bar");
        })
    });

    handle.assert_finished();
}

#[test]
fn explicit_child() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(span::mock().named("foo"))
        .new_span(span::mock().named("bar").with_explicit_parent(Some("foo")))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        let foo = span!("foo");
        span!(parent: foo.id(), "bar");
    });

    handle.assert_finished();
}

#[test]
fn explicit_child_regardless_of_ctx() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(span::mock().named("foo"))
        .new_span(span::mock().named("bar"))
        .enter(span::mock().named("bar"))
        .new_span(span::mock().named("baz").with_explicit_parent(Some("foo")))
        .exit(span::mock().named("bar"))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        let foo = span!("foo");
        span!("bar").enter(|| span!(parent: foo.id(), "baz"))
    });

    handle.assert_finished();
}

#[test]
fn contextual_root() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(span::mock().named("foo").with_contextual_parent(None))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        span!("foo");
    });

    handle.assert_finished();
}

#[test]
fn contextual_child() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .new_span(
            span::mock()
                .named("bar")
                .with_contextual_parent(Some("foo")),
        )
        .exit(span::mock().named("foo"))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        span!("foo").enter(|| {
            span!("bar");
        })
    });

    handle.assert_finished();
}
