#[macro_use]
extern crate tokio_trace;
mod support;

use self::support::*;
use tokio_trace::{dispatcher, Dispatch};

#[test]
fn dispatcher_is_sticky() {
    // Test ensuring that entire trace trees are collected by the same
    // dispatcher, even across dispatcher context switches.
    let (subscriber1, handle1) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .enter(span::mock().named("bar"))
        .exit(span::mock().named("bar"))
        .drop_span(span::mock().named("bar"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    let mut foo = dispatcher::with_default(Dispatch::new(subscriber1), || {
        let mut foo = span!("foo");
        foo.enter(|| {});
        foo
    });
    dispatcher::with_default(Dispatch::new(subscriber::mock().done().run()), move || {
        foo.enter(|| span!("bar").enter(|| {}))
    });

    handle1.assert_finished();
}

#[test]
fn dispatcher_isnt_too_sticky() {
    // Test ensuring that new trace trees are collected by the current
    // dispatcher.
    let (subscriber1, handle1) = subscriber::mock()
        .enter(span::mock().named("foo"))
        .exit(span::mock().named("foo"))
        .enter(span::mock().named("foo"))
        .enter(span::mock().named("bar"))
        .exit(span::mock().named("bar"))
        .drop_span(span::mock().named("bar"))
        .exit(span::mock().named("foo"))
        .drop_span(span::mock().named("foo"))
        .done()
        .run_with_handle();
    let (subscriber2, handle2) = subscriber::mock()
        .enter(span::mock().named("baz"))
        .enter(span::mock().named("quux"))
        .exit(span::mock().named("quux"))
        .drop_span(span::mock().named("quux"))
        .exit(span::mock().named("baz"))
        .drop_span(span::mock().named("baz"))
        .done()
        .run_with_handle();

    let mut foo = dispatcher::with_default(Dispatch::new(subscriber1), || {
        let mut foo = span!("foo");
        foo.enter(|| {});
        foo
    });
    let mut baz = dispatcher::with_default(Dispatch::new(subscriber2), || span!("baz"));
    dispatcher::with_default(Dispatch::new(subscriber::mock().done().run()), move || {
        foo.enter(|| span!("bar").enter(|| {}));
        baz.enter(|| span!("quux").enter(|| {}))
    });

    handle1.assert_finished();
    handle2.assert_finished();
}
