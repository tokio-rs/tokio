#[macro_use]
extern crate tokio_trace;
mod support;

use self::support::*;

use tokio_trace::subscriber::with_default;

#[test]
fn event_without_message() {
    let (subscriber, handle) = subscriber::mock()
        .event(event::mock().with_fields(
            field::mock("answer")
                .with_value(&42)
                .and(field::mock("to_question")
                    .with_value(&"life, the universe, and everything"))
                .only()
        ))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        info!(answer = 42, to_question = "life, the universe, and everything");
    });

    handle.assert_finished();
}

#[test]
fn event_with_message() {
    let (subscriber, handle) = subscriber::mock()
        .event(event::mock().with_fields(
            field::mock("message")
                .with_value(&tokio_trace::field::debug(
                    format_args!("hello from my event! yak shaved = {:?}", true)
                ))
        ))
        .done()
        .run_with_handle();

    with_default(subscriber, || {
        debug!("hello from my event! yak shaved = {:?}", true);
    });

    handle.assert_finished();
}
