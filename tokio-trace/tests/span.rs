#[macro_use]
extern crate tokio_trace;
mod support;

use self::support::*;
use std::thread;
use tokio_trace::{
    field::{debug, display},
    subscriber::with_default,
    Level, Span,
};

#[test]
fn handles_to_the_same_span_are_equal() {
    // Create a mock subscriber that will return `true` on calls to
    // `Subscriber::enabled`, so that the spans will be constructed. We
    // won't enter any spans in this test, so the subscriber won't actually
    // expect to see any spans.
    with_default(subscriber::mock().run(), || {
        let foo1 = span!(Level::TRACE, "foo");
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
        let foo1 = span!(Level::TRACE, "foo", bar = 1u64, baz = false);
        let foo2 = span!(Level::TRACE, "foo", bar = 1u64, baz = false);

        assert_ne!(foo1, foo2);
    });
}

#[test]
fn handles_to_different_spans_with_the_same_metadata_are_not_equal() {
    // Every time time this function is called, it will return a _new
    // instance_ of a span with the same metadata, name, and fields.
    fn make_span() -> Span {
        span!(Level::TRACE, "foo", bar = 1u64, baz = false)
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

    let foo = with_default(subscriber1, || {
        let foo = span!(Level::TRACE, "foo");
        foo.in_scope(|| {});
        foo
    });
    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    with_default(subscriber2, move || foo.in_scope(|| {}));
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
    let foo = with_default(subscriber1, || {
        let foo = span!(Level::TRACE, "foo");
        foo.in_scope(|| {});
        foo
    });

    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    thread::spawn(move || {
        with_default(subscriber::mock().run(), || {
            foo.in_scope(|| {});
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
        let span = span!(Level::TRACE, "foo");
        span.in_scope(|| {});
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
        span!(Level::TRACE, "foo").in_scope(|| {
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
        span!(Level::TRACE, "foo").in_scope(|| {
            event!(Level::DEBUG, {}, "my event!");
        });
        span!(Level::TRACE, "bar").in_scope(|| {});
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
        span!(Level::TRACE, "foo").in_scope(|| {});
    });

    handle.assert_finished();
}

#[test]
fn cloning_a_span_calls_clone_span() {
    let (subscriber, handle) = subscriber::mock()
        .clone_span(span::mock().named("foo"))
        .run_with_handle();
    with_default(subscriber, || {
        let span = span!(Level::TRACE, "foo");
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
        let span = span!(Level::TRACE, "foo");
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

    let foo = with_default(subscriber1, || {
        let foo = span!(Level::TRACE, "foo");
        foo.in_scope(|| {});
        foo
    });
    // Even though we enter subscriber 2's context, the subscriber that
    // tagged the span should see the enter/exit.
    with_default(subscriber2, move || {
        let foo2 = foo.clone();
        foo.in_scope(|| {});
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
        let foo = span!(Level::TRACE, "foo");

        foo.in_scope(|| {});

        drop(foo);
    });

    handle.assert_finished();
}

#[test]
fn enter() {
    let (subscriber, handle) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .event(event::mock())
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        let foo = span!(Level::TRACE, "foo");
        let _enter = foo.enter();
        debug!("dropping guard...");
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
        let span = span!(
            Level::TRACE,
            "foo",
            bar = display(format!("hello from {}", from))
        );
        span.in_scope(|| {});
    });

    handle.assert_finished();
}

#[test]
fn dotted_field_name() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock()
                .named("foo")
                .with_field(field::mock("fields.bar").with_value(&true).only()),
        )
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        span!(Level::TRACE, "foo", fields.bar = true);
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
        let span = span!(Level::TRACE, "foo", bar = display(&message));
        span.in_scope(|| {
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
        let foo = span!(Level::TRACE, "foo", x = debug(pos.x), y = debug(pos.y));
        let bar = span!(Level::TRACE, "bar", position = debug(pos));
        foo.in_scope(|| {});
        bar.in_scope(|| {});
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
        let span = span!(Level::TRACE, "foo", bar = 5, baz);
        span.record("baz", &true);
        span.in_scope(|| {})
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
        let span = span!(Level::TRACE, "foo", bar, baz);
        span.record("bar", &5);
        span.record("baz", &true);
        span.in_scope(|| {})
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
                .at_level(Level::DEBUG),
        )
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        span!(Level::DEBUG, target: "app_span", "foo");
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
        span!(Level::TRACE, parent: None, "foo");
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
        span!(Level::TRACE, "foo").in_scope(|| {
            span!(Level::TRACE, parent: None, "bar");
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
        let foo = span!(Level::TRACE, "foo");
        span!(Level::TRACE, parent: foo.id(), "bar");
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
        let foo = span!(Level::TRACE, "foo");
        span!(Level::TRACE, "bar").in_scope(|| span!(Level::TRACE, parent: foo.id(), "baz"))
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
        span!(Level::TRACE, "foo");
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
        span!(Level::TRACE, "foo").in_scope(|| {
            span!(Level::TRACE, "bar");
        })
    });

    handle.assert_finished();
}

#[test]
fn display_shorthand() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock().named("my_span").with_field(
                field::mock("my_field")
                    .with_value(&display("hello world"))
                    .only(),
            ),
        )
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        span!(Level::TRACE, "my_span", my_field = %"hello world");
    });

    handle.assert_finished();
}

#[test]
fn debug_shorthand() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock().named("my_span").with_field(
                field::mock("my_field")
                    .with_value(&debug("hello world"))
                    .only(),
            ),
        )
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        span!(Level::TRACE, "my_span", my_field = ?"hello world");
    });

    handle.assert_finished();
}

#[test]
fn both_shorthands() {
    let (subscriber, handle) = subscriber::mock()
        .new_span(
            span::mock().named("my_span").with_field(
                field::mock("display_field")
                    .with_value(&display("hello world"))
                    .and(field::mock("debug_field").with_value(&debug("hello world")))
                    .only(),
            ),
        )
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        span!(Level::TRACE, "my_span", display_field = %"hello world", debug_field = ?"hello world");
    });

    handle.assert_finished();
}
