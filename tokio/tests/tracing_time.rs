//! Tests for time resource instrumentation.
//!
//! These tests ensure that the instrumentation for tokio
//! synchronization primitives is correct.
#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, feature = "tracing", target_has_atomic = "64"))]

use std::time::Duration;

use tracing_mock::{expect, subscriber};

#[tokio::test]
async fn test_sleep_creates_span() {
    let sleep_span_id = expect::id();
    let sleep_span = expect::span()
        .with_id(sleep_span_id.clone())
        .named("runtime.resource")
        .with_target("tokio::time::sleep");

    let poll_op = || {
        expect::event()
            .with_target("runtime::resource::poll_op")
            .with_fields(expect::field("op_name").with_value(&"poll_elapsed"))
    };

    let state_update = expect::event()
        .with_target("runtime::resource::state_update")
        .with_fields(
            expect::field("duration")
                // FIXME(hds): This value isn't stable and doesn't seem to make sense. We're not
                //             going to test on it until the resource instrumentation has been
                //             refactored and we know that we're getting a stable value here.
                //.with_value(&(7_u64 + 1))
                .and(expect::field("duration.op").with_value(&"override")),
        );

    let async_op_span_id = expect::id();
    let async_op_span = expect::span()
        .with_id(async_op_span_id.clone())
        .named("runtime.resource.async_op")
        .with_target("tokio::time::sleep");

    let async_op_poll_span = expect::span()
        .named("runtime.resource.async_op.poll")
        .with_target("tokio::time::sleep");

    let (subscriber, handle) = subscriber::mock()
        .new_span(sleep_span.clone().with_ancestry(expect::is_explicit_root()))
        .new_span(
            async_op_span
                .clone()
                .with_ancestry(expect::has_explicit_parent(&sleep_span_id))
                .with_fields(expect::field("source").with_value(&"Sleep::new_timeout")),
        )
        .new_span(
            async_op_poll_span
                .clone()
                .with_ancestry(expect::has_explicit_parent(&async_op_span_id)),
        )
        .enter(sleep_span.clone())
        .enter(async_op_span.clone())
        .enter(async_op_poll_span.clone())
        .event(poll_op())
        .event(state_update)
        .event(poll_op())
        .exit(async_op_poll_span.clone())
        .drop_span(async_op_poll_span)
        .exit(async_op_span.clone())
        .drop_span(async_op_span)
        .exit(sleep_span.clone())
        .drop_span(sleep_span)
        .run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);

        tokio::time::sleep(Duration::from_millis(7)).await;
    }

    handle.assert_finished();
}
