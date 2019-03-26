#[macro_use]
extern crate tokio_trace;
mod support;

use self::support::*;

use tokio_trace::{field::display, subscriber::with_default, Level};

#[test]
fn event_without_message() {
    let (subscriber, handle) = subscriber::mock()
        .event(
            event::mock().with_fields(
                field::mock("answer")
                    .with_value(&42)
                    .and(
                        field::mock("to_question")
                            .with_value(&"life, the universe, and everything"),
                    )
                    .only(),
            ),
        )
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        info!(
            answer = 42,
            to_question = "life, the universe, and everything"
        );
    });

    handle.assert_finished();
}

#[test]
fn event_with_message() {
    let (subscriber, handle) = subscriber::mock()
        .event(event::mock().with_fields(field::mock("message").with_value(
            &tokio_trace::field::debug(format_args!(
                "hello from my event! yak shaved = {:?}",
                true
            )),
        )))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        debug!("hello from my event! yak shaved = {:?}", true);
    });

    handle.assert_finished();
}

#[test]
fn one_with_everything() {
    let (subscriber, handle) = subscriber::mock()
        .event(
            event::mock()
                .with_fields(
                    field::mock("message")
                        .with_value(&tokio_trace::field::debug(format_args!(
                            "{:#x} make me one with{what:.>20}",
                            4277009102u64,
                            what = "everything"
                        )))
                        .and(field::mock("foo").with_value(&666))
                        .and(field::mock("bar").with_value(&false))
                        .only(),
                )
                .at_level(tokio_trace::Level::ERROR)
                .with_target("whatever"),
        )
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        event!(
            target: "whatever",
            tokio_trace::Level::ERROR,
            { foo = 666, bar = false },
             "{:#x} make me one with{what:.>20}", 4277009102u64, what = "everything"
        );
    });

    handle.assert_finished();
}

#[test]
fn moved_field() {
    let (subscriber, handle) = subscriber::mock()
        .event(
            event::mock().with_fields(
                field::mock("foo")
                    .with_value(&display("hello from my event"))
                    .only(),
            ),
        )
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        let from = "my event";
        event!(Level::INFO, foo = display(format!("hello from {}", from)))
    });

    handle.assert_finished();
}

#[test]
fn borrowed_field() {
    let (subscriber, handle) = subscriber::mock()
        .event(
            event::mock().with_fields(
                field::mock("foo")
                    .with_value(&display("hello from my event"))
                    .only(),
            ),
        )
        .done()
        .run_with_handle();
    with_default(subscriber, || {
        let from = "my event";
        let mut message = format!("hello from {}", from);
        event!(Level::INFO, foo = display(&message));
        message.push_str(", which happened!");
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
        .event(
            event::mock().with_fields(
                field::mock("x")
                    .with_value(&debug(3.234))
                    .and(field::mock("y").with_value(&debug(-1.223)))
                    .only(),
            ),
        )
        .event(event::mock().with_fields(field::mock("position").with_value(&debug(&pos))))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        let pos = Position {
            x: 3.234,
            y: -1.223,
        };
        debug!(x = debug(pos.x), y = debug(pos.y));
        debug!(target: "app_events", { position = debug(pos) }, "New position");
    });
    handle.assert_finished();
}
