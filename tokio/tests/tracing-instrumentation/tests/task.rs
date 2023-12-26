//! Tests for task instrumentation.
//!
//! These tests ensure that the instrumentation for task spawning and task
//! lifecycles is correct.

use tokio::task;
use tracing_mock::{expect, span::NewSpan, subscriber};

#[tokio::test]
async fn task_spawn_creates_span() {
    let task_span = expect::span()
        .named("runtime.spawn")
        .with_target("tokio::task");

    let (subscriber, handle) = subscriber::mock()
        .new_span(task_span.clone())
        .enter(task_span.clone())
        .exit(task_span.clone())
        // The task span is entered once more when it gets dropped
        .enter(task_span.clone())
        .exit(task_span.clone())
        .drop_span(task_span)
        .run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        tokio::spawn(futures::future::ready(()))
            .await
            .expect("failed to await join handle");
    }

    handle.assert_finished();
}

#[tokio::test]
async fn task_spawn_loc_file_recorded() {
    let task_span = expect::span()
        .named("runtime.spawn")
        .with_target("tokio::task")
        .with_field(expect::field("loc.file").with_value(&file!()));

    let (subscriber, handle) = subscriber::mock().new_span(task_span).run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);

        tokio::spawn(futures::future::ready(()))
            .await
            .expect("failed to await join handle");
    }

    handle.assert_finished();
}

#[tokio::test]
async fn task_builder_name_recorded() {
    let task_span = expect_task_named("test-task");

    let (subscriber, handle) = subscriber::mock().new_span(task_span).run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        task::Builder::new()
            .name("test-task")
            .spawn(futures::future::ready(()))
            .unwrap()
            .await
            .expect("failed to await join handle");
    }

    handle.assert_finished();
}

#[tokio::test]
async fn task_builder_loc_file_recorded() {
    let task_span = expect::span()
        .named("runtime.spawn")
        .with_target("tokio::task")
        .with_field(expect::field("loc.file").with_value(&file!()));

    let (subscriber, handle) = subscriber::mock().new_span(task_span).run_with_handle();

    {
        let _guard = tracing::subscriber::set_default(subscriber);

        task::Builder::new()
            .spawn(futures::future::ready(()))
            .unwrap()
            .await
            .expect("failed to await join handle");
    }

    handle.assert_finished();
}

/// Expect a task with name
///
/// This is a convenience function to create the expectation for a new task
/// with the `task.name` field set to the provided name.
fn expect_task_named(name: &str) -> NewSpan {
    expect::span()
        .named("runtime.spawn")
        .with_target("tokio::task")
        .with_field(
            expect::field("task.name").with_value(&tracing::field::debug(format_args!("{}", name))),
        )
}
