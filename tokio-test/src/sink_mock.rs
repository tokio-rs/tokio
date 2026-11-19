#![cfg(not(loom))]

//! A mock sink implementing [`Sink`].
//!
//! # Overview
//! This module provides a `SinkMock` that can be used to test code that interacts with sinks.
//! It allows you to specify the items the sink expects to receive, the waiting intervals between
//! them and the errors the sink returns.
//!
//! # Usage
//! To use the `SinkMock`, you need to create a builder using [`SinkMockBuilder`]. The builder
//! allows you to enqueue actions such as expecting items, waiting for a certain duration or
//! returning errors. The actions are processed in order.
//!
//! The sink panics if it receives an item it does not expect, or if it is dropped before it has
//! received all expected items and returned all errors.
//!
//! # Example
//! ```rust
//! use futures_util::{Sink, SinkExt};
//! use std::io;
//! use std::time::Duration;
//! use tokio_test::sink_mock::SinkMockBuilder;
//!
//! // The code under test.
//! async fn greet<S>(mut sink: S) -> io::Result<()>
//! where
//!     S: Sink<&'static str, Error = io::Error> + Unpin,
//! {
//!     sink.send("hello").await?;
//!     sink.send("world").await?;
//!     sink.close().await
//! }
//!
//! # #[tokio::main(flavor = "current_thread", start_paused = true)]
//! # async fn main() {
//! let sink_mock = SinkMockBuilder::new()
//!     .send("hello")
//!     .wait(Duration::from_millis(300))
//!     .send("world")
//!     .build();
//! greet(sink_mock).await.unwrap();
//!
//! let sink_mock = SinkMockBuilder::new()
//!     .send("hello")
//!     .error(io::Error::other("connection reset"))
//!     .build();
//! assert!(greet(sink_mock).await.is_err());
//! # }
//! ```

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::Duration;

use futures_sink::Sink;
use tokio::time::{self, Sleep};

#[derive(Debug, Clone)]
enum Action<T, E> {
    Send(T),
    Wait(Duration),
    Error(E),
}

/// A builder for [`SinkMock`]
#[derive(Debug, Clone)]
pub struct SinkMockBuilder<T, E> {
    actions: VecDeque<Action<T, E>>,
}

impl<T, E> SinkMockBuilder<T, E> {
    /// Create a new empty [`SinkMockBuilder`]
    pub fn new() -> Self {
        SinkMockBuilder::default()
    }

    /// Queue an item to be received by the sink
    ///
    /// The sink panics if it receives a different item.
    pub fn send(mut self, value: T) -> Self {
        self.actions.push_back(Action::Send(value));
        self
    }

    /// Queue the sink to wait for a duration
    ///
    /// The next call to `poll_ready`, `poll_flush` or `poll_close` returns
    /// `Poll::Pending` until the duration has elapsed.
    pub fn wait(mut self, duration: Duration) -> Self {
        self.actions.push_back(Action::Wait(duration));
        self
    }

    /// Queue an error to be returned by the sink
    ///
    /// The next call to `poll_ready`, `poll_flush` or `poll_close` returns
    /// the error.
    pub fn error(mut self, error: E) -> Self {
        self.actions.push_back(Action::Error(error));
        self
    }

    /// Build the [`SinkMock`]
    pub fn build(self) -> SinkMock<T, E> {
        SinkMock {
            actions: self.actions,
            sleep: None,
        }
    }
}

impl<T, E> Default for SinkMockBuilder<T, E> {
    fn default() -> Self {
        SinkMockBuilder {
            actions: VecDeque::new(),
        }
    }
}

/// A mock sink implementing [`Sink`]
///
/// See [`SinkMockBuilder`] for more information.
#[derive(Debug)]
pub struct SinkMock<T, E> {
    actions: VecDeque<Action<T, E>>,
    sleep: Option<Pin<Box<Sleep>>>,
}

// We never put `T` or `E` in a `Pin`.
impl<T, E> Unpin for SinkMock<T, E> {}

impl<T, E> SinkMock<T, E> {
    /// Handles the waits and the error at the front of the queue.
    fn poll_actions(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), E>> {
        loop {
            if let Some(sleep) = &mut self.sleep {
                ready!(sleep.as_mut().poll(cx));
                self.sleep = None;
            }

            match self.actions.pop_front() {
                Some(Action::Wait(duration)) => self.sleep = Some(Box::pin(time::sleep(duration))),
                Some(Action::Error(error)) => return Poll::Ready(Err(error)),
                Some(action) => {
                    // Leave the expected item for `start_send`.
                    self.actions.push_front(action);
                    return Poll::Ready(Ok(()));
                }
                None => return Poll::Ready(Ok(())),
            }
        }
    }
}

impl<T: PartialEq + fmt::Debug, E> Sink<T> for SinkMock<T, E> {
    type Error = E;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), E>> {
        self.get_mut().poll_actions(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), E> {
        match self.get_mut().actions.pop_front() {
            Some(Action::Send(expected)) if item == expected => Ok(()),
            Some(Action::Send(expected)) => {
                panic!("SinkMock received unexpected item {item:?}, expected {expected:?}")
            }
            Some(_) => panic!(
                "SinkMock received item {item:?} before `poll_ready` returned `Ready(Ok(()))`"
            ),
            None => {
                panic!("SinkMock received unexpected item {item:?}, no more items were expected")
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), E>> {
        self.get_mut().poll_actions(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), E>> {
        self.get_mut().poll_actions(cx)
    }
}

impl<T, E> Drop for SinkMock<T, E> {
    fn drop(&mut self) {
        // Avoid double panicking to make debugging easier.
        if std::thread::panicking() {
            return;
        }

        let unconsumed_count = self
            .actions
            .iter()
            .filter(|action| !matches!(action, Action::Wait(_)))
            .count();

        assert!(
            unconsumed_count == 0,
            "SinkMock was dropped before all actions were consumed, {unconsumed_count} actions were not consumed"
        );
    }
}
