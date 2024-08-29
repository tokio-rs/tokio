#![cfg(not(loom))]

//! A mock stream implementing [`Stream`].
//!
//! # Overview
//! This crate provides a `StreamMock` that can be used to test code that interacts with streams.
//! It allows you to mock the behavior of a stream and control the items it yields and the waiting
//! intervals between items.
//!
//! # Usage
//! To use the `StreamMock`, you need to create a builder using [`StreamMockBuilder`]. The builder
//! allows you to enqueue actions such as returning items or waiting for a certain duration.
//!
//! # Example
//! ```rust
//!
//! use futures_util::StreamExt;
//! use std::time::Duration;
//! use tokio_test::stream_mock::StreamMockBuilder;
//!
//! async fn test_stream_mock_wait() {
//!     let mut stream_mock = StreamMockBuilder::new()
//!         .next(1)
//!         .wait(Duration::from_millis(300))
//!         .next(2)
//!         .build();
//!
//!     assert_eq!(stream_mock.next().await, Some(1));
//!     let start = std::time::Instant::now();
//!     assert_eq!(stream_mock.next().await, Some(2));
//!     let elapsed = start.elapsed();
//!     assert!(elapsed >= Duration::from_millis(300));
//!     assert_eq!(stream_mock.next().await, None);
//! }
//! ```

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{ready, Poll};
use std::time::Duration;

use futures_core::Stream;
use std::future::Future;
use tokio::time::{sleep_until, Instant, Sleep};

#[derive(Debug, Clone)]
enum Action<T: Unpin> {
    Next(T),
    Wait(Duration),
}

/// A builder for [`StreamMock`]
#[derive(Debug, Clone)]
pub struct StreamMockBuilder<T: Unpin> {
    actions: VecDeque<Action<T>>,
}

impl<T: Unpin> StreamMockBuilder<T> {
    /// Create a new empty [`StreamMockBuilder`]
    pub fn new() -> Self {
        StreamMockBuilder::default()
    }

    /// Queue an item to be returned by the stream
    pub fn next(mut self, value: T) -> Self {
        self.actions.push_back(Action::Next(value));
        self
    }

    // Queue an item to be consumed by the sink,
    // commented out until Sink is implemented.
    //
    // pub fn consume(mut self, value: T) -> Self {
    //    self.actions.push_back(Action::Consume(value));
    //    self
    // }

    /// Queue the stream to wait for a duration
    pub fn wait(mut self, duration: Duration) -> Self {
        self.actions.push_back(Action::Wait(duration));
        self
    }

    /// Build the [`StreamMock`]
    pub fn build(self) -> StreamMock<T> {
        StreamMock {
            actions: self.actions,
            sleep: None,
        }
    }
}

impl<T: Unpin> Default for StreamMockBuilder<T> {
    fn default() -> Self {
        StreamMockBuilder {
            actions: VecDeque::new(),
        }
    }
}

/// A mock stream implementing [`Stream`]
///
/// See [`StreamMockBuilder`] for more information.
#[derive(Debug)]
pub struct StreamMock<T: Unpin> {
    actions: VecDeque<Action<T>>,
    sleep: Option<Pin<Box<Sleep>>>,
}

impl<T: Unpin> StreamMock<T> {
    fn next_action(&mut self) -> Option<Action<T>> {
        self.actions.pop_front()
    }
}

impl<T: Unpin> Stream for StreamMock<T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // Try polling the sleep future first
        if let Some(ref mut sleep) = self.sleep {
            ready!(Pin::new(sleep).poll(cx));
            // Since we're ready, discard the sleep future
            self.sleep.take();
        }

        match self.next_action() {
            Some(action) => match action {
                Action::Next(item) => Poll::Ready(Some(item)),
                Action::Wait(duration) => {
                    // Set up a sleep future and schedule this future to be polled again for it.
                    self.sleep = Some(Box::pin(sleep_until(Instant::now() + duration)));
                    cx.waker().wake_by_ref();

                    Poll::Pending
                }
            },
            None => Poll::Ready(None),
        }
    }
}

impl<T: Unpin> Drop for StreamMock<T> {
    fn drop(&mut self) {
        // Avoid double panicking to make debugging easier.
        if std::thread::panicking() {
            return;
        }

        let undropped_count = self
            .actions
            .iter()
            .filter(|action| match action {
                Action::Next(_) => true,
                Action::Wait(_) => false,
            })
            .count();

        assert!(
            undropped_count == 0,
            "StreamMock was dropped before all actions were consumed, {} actions were not consumed",
            undropped_count
        );
    }
}
