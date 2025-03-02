#![cfg(not(loom))]

//! A mock sink implementing [`Sink`].
//!
//! # Overview
//! This module provides a `SinkMock` that can be used to test code that interacts with sinks.
//! It allows you to mock the behavior of a sink and control the items it expects and the waiting
//! intervals required between items.
//!
//! # Usage
//! To use the `SinkMock`, you need to create a builder using [`SinkMockBuilder`].
//! The builder allows you to enqueue actions such as
//! requiring items or requiring a pause between items.
//!
//! # Example
//!
//! ```rust
//! use tokio_test::sink_mock::SinkMockBuilder;
//! use futures_util::SinkExt;
//! use std::time::Duration;
//!
//! async fn test_sink_mock_wait() {
//!     let mut sink_mock = SinkMockBuilder::new()
//!         .require(1)
//!         .require_wait(Duration::from_millis(300))
//!         .require(2)
//!         .build();
//!
//!     assert_eq!(sink_mock.send(1).await, Ok(()));
//!     tokio::time::sleep(Duration::from_millis(300)).await;
//!     assert_eq!(sink_mock.send(2).await, Ok(()));
//! }
//! ```

use std::{
    collections::VecDeque,
    pin::Pin,
    task::Poll,
    time::{Duration, Instant},
};

use futures_sink::Sink;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action<T, E> {
    Consume(T),
    ConsumeWithError(T, E),
    Pause(Duration),
}

/// A builder for [`SinkMock`].
#[derive(Debug, Clone)]
pub struct SinkMockBuilder<T, E> {
    actions: VecDeque<Action<T, E>>,
}

impl<T: Unpin, E: Unpin> SinkMockBuilder<T, E> {
    /// Create a new empty [`SinkMockBuilder`].
    pub fn new() -> Self {
        SinkMockBuilder::default()
    }

    /// Queue an item to be required by the [`Sink`].
    pub fn require(mut self, value: T) -> Self {
        self.actions.push_back(Action::Consume(value));
        self
    }

    /// Queue an item to be required by the [`Sink`],
    /// which shall produce the given error when polled.
    pub fn require_with_error(mut self, value: T, error: E) -> Self {
        let action = Action::ConsumeWithError(value, error);
        self.actions.push_back(action);
        self
    }

    /// Queue the sink to require waiting for a while before receiving another value.
    pub fn require_wait(mut self, duration: Duration) -> Self {
        self.actions.push_back(Action::Pause(duration));
        self
    }

    /// Build the [`SinkMock`].
    pub fn build(self) -> SinkMock<T, E> {
        SinkMock {
            actions: self.actions,
            last_action: Instant::now(),
        }
    }
}

impl<T: Unpin, E: Unpin> Default for SinkMockBuilder<T, E> {
    fn default() -> Self {
        SinkMockBuilder {
            actions: VecDeque::new(),
        }
    }
}

/// A mock sink implementing [`Sink`].
///
/// See [`SinkMockBuilder`] for more information.
#[derive(Debug)]
pub struct SinkMock<T, E> {
    actions: VecDeque<Action<T, E>>,
    last_action: Instant,
}

impl<T: Unpin + Eq + std::fmt::Debug, E: Unpin> Sink<T> for SinkMock<T, E> {
    type Error = E;

    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Requires `Eq + std::fmt::Debug` due to usage of `assert_eq!`.
    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        loop {
            let Some(action) = self.actions.pop_front() else {
                panic!("Sink does not expect any items");
            };
            match action {
                Action::Pause(duration) => {
                    let now = Instant::now();
                    if (self.last_action + duration) <= now {
                        self.last_action = now;
                        continue;
                    } else {
                        panic!("Sink received item too early");
                    }
                }
                Action::Consume(queued_item) => {
                    assert_eq!(item, queued_item);
                    self.last_action = Instant::now();
                    break Ok(());
                }
                Action::ConsumeWithError(queued_item, queued_error) => {
                    assert_eq!(item, queued_item);
                    self.last_action = Instant::now();
                    break Err(queued_error);
                }
            }
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.try_close();
        Poll::Ready(Ok(()))
    }
}

impl<T, E> Drop for SinkMock<T, E> {
    fn drop(&mut self) {
        // Avoid double panicking to make debugging easier.
        if std::thread::panicking() {
            return;
        }
        self.try_close();
    }
}

impl<T, E> SinkMock<T, E> {
    fn try_close(&mut self) {
        loop {
            let Some(action) = self.actions.pop_front() else {
                break;
            };
            match action {
                Action::Pause(duration) => {
                    let now = Instant::now();
                    if (self.last_action + duration) <= now {
                        self.last_action += duration;
                        continue;
                    } else {
                        panic!("Sink closed too early");
                    }
                }
                Action::Consume(..) | Action::ConsumeWithError(..) => {
                    panic!("Sink expects more items")
                }
            }
        }
    }
}

#[cfg(test)]
mod test {

    use crate::sink_mock::{SinkMock, SinkMockBuilder};
    use futures_util::SinkExt;
    use std::time::Duration;

    #[test]
    #[should_panic(expected = "Sink expects more items")]
    fn dropping_nonempty_sink_panics() {
        let sink_mock: SinkMock<i32, ()> = SinkMockBuilder::new().require(1).build();
        drop(sink_mock);
    }

    #[tokio::test]
    #[should_panic(expected = "Sink does not expect any items")]
    async fn empty_sink_panics_on_send() {
        let mut sink_mock: SinkMock<i32, ()> = SinkMockBuilder::new().build();
        let _ = sink_mock.send(1).await;
    }

    #[tokio::test]
    #[should_panic(expected = "Sink received item too early")]
    async fn should_reject_values_when_sent_too_early() {
        let mut sink_mock: SinkMock<i32, ()> = SinkMockBuilder::new()
            .require_wait(Duration::from_secs(1))
            .build();

        sink_mock.send(1).await.unwrap();
    }

    #[test]
    #[should_panic(expected = "Sink closed too early")]
    fn paused_sink_panics_on_drop() {
        let sink_mock: SinkMock<i32, ()> = SinkMockBuilder::new()
            .require_wait(Duration::from_secs(1))
            .build();

        drop(sink_mock);
    }

    #[tokio::test]
    #[should_panic(expected = "Sink closed too early")]
    async fn paused_sink_panics_on_close() {
        let mut sink_mock: SinkMock<i32, ()> = SinkMockBuilder::new()
            .require_wait(Duration::from_secs(1))
            .build();

        sink_mock.close().await.unwrap();
    }

    #[tokio::test]
    async fn should_yield_error() {
        let mut sink_mock = SinkMockBuilder::new()
            .require_with_error(1, "oh no")
            .require_with_error(2, "well...")
            .require_wait(Duration::from_millis(500))
            .require_with_error(3, "ok.")
            .build();

        assert_eq!(sink_mock.send(1).await.unwrap_err(), "oh no");
        assert_eq!(sink_mock.send(2).await.unwrap_err(), "well...");
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(sink_mock.send(3).await.unwrap_err(), "ok.");
    }

    #[tokio::test]
    async fn should_sum_pause_durations() {
        let mut sink_mock = SinkMockBuilder::<i32, ()>::new()
            .require_wait(Duration::from_millis(1))
            .require_wait(Duration::from_millis(1))
            .require_wait(Duration::from_millis(1))
            .build();

        tokio::time::sleep(Duration::from_millis(3)).await;

        let _ = sink_mock.close().await;
    }

    #[tokio::test]
    async fn should_require_value_after_waiting() {
        let mut sink_mock: SinkMock<i32, ()> = SinkMockBuilder::new()
            .require(1)
            .require_wait(Duration::from_millis(300))
            .require(3)
            .build();

        sink_mock.send(1).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        sink_mock.send(3).await.unwrap();
    }
}
