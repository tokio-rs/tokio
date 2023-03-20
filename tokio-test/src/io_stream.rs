#![cfg(not(loom))]

//! A mock type implementing [`poll_next`].
//!
//!
//! # Overview
//!
//!  TODO
//!
//! # Usage
//!
//! Attempting to write data that the mock isn't expecting will result in a
//! panic.
//!
//! [`AsyncRead`]: tokio::io::AsyncRead
//! [`AsyncWrite`]: tokio::io::AsyncWrite


use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, Sleep};
use tokio_stream::wrappers::UnboundedReceiverStream;
use futures_core::{ready, Stream};
use std::future::Future;
use std::collections::VecDeque;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{self, Context, Poll, Waker};
use std::{cmp, io};

/// An I/O object that follows a predefined script.
///
/// This value is created by `Builder` and implements `AsyncRead` + `AsyncWrite`. It
/// follows the scenario described by the builder and panics otherwise.
#[derive(Debug)]
pub struct Mock {
    inner: Inner,
}

/// A handle to send additional actions to the related `Mock`.
#[derive(Debug)]
pub struct Handle {
    tx: mpsc::UnboundedSender<Action>,
}

/// Builds `Mock` instances.
#[derive(Debug, Clone, Default)]
pub struct StreamBuilder {
    // Sequence of actions for the Mock to take
    actions: VecDeque<Action>,
}

#[derive(Debug, Clone)]
enum Action {
    Read(Vec<u8>),
    Write(Vec<u8>),
    Wait(Duration),
    Next(String),
    // Wrapped in Arc so that Builder can be cloned and Send.
    // Mock is not cloned as does not need to check Rc for ref counts.
    ReadError(Option<Arc<io::Error>>),
    WriteError(Option<Arc<io::Error>>),
}

struct Inner {
    actions: VecDeque<Action>,
    waiting: Option<Instant>,
    sleep: Option<Pin<Box<Sleep>>>,
    read_wait: Option<Waker>,
    rx: UnboundedReceiverStream<Action>,
}

impl StreamBuilder {
    /// Return a new, empty `Builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sequence a `read` operation.
    ///
    /// The next operation in the mock's script will be to expect a `read` call
    /// and return `buf`.
    pub fn read(&mut self, buf: &[u8]) -> &mut Self {
        self.actions.push_back(Action::Read(buf.into()));
        self
    }

    /// Sequence a `read` operation.
    ///
    /// The next operation in the mock's script will be to expect a `read` call
    /// and return `buf`.
    pub fn read_stream(&mut self, string: String) -> &mut Self {
        self.actions.push_back(Action::Next(string));
        self
    }

    /// Sequence a `read` operation that produces an error.
    ///
    /// The next operation in the mock's script will be to expect a `read` call
    /// and return `error`.
    pub fn read_error(&mut self, error: io::Error) -> &mut Self {
        let error = Some(error.into());
        self.actions.push_back(Action::ReadError(error));
        self
    }

    /// Sequence a `write` operation.
    ///
    /// The next operation in the mock's script will be to expect a `write`
    /// call.
    pub fn write(&mut self, buf: &[u8]) -> &mut Self {
        self.actions.push_back(Action::Write(buf.into()));
        self
    }

    /// Sequence a `write` operation that produces an error.
    ///
    /// The next operation in the mock's script will be to expect a `write`
    /// call that provides `error`.
    pub fn write_error(&mut self, error: io::Error) -> &mut Self {
        let error = Some(error.into());
        self.actions.push_back(Action::WriteError(error));
        self
    }

    /// Sequence a wait.
    ///
    /// The next operation in the mock's script will be to wait without doing so
    /// for `duration` amount of time.
    pub fn wait(&mut self, duration: Duration) -> &mut Self {
        let duration = cmp::max(duration, Duration::from_millis(1));
        self.actions.push_back(Action::Wait(duration));
        self
    }
    /// calls next value within stream.
    ///

    /// Build a `Mock` value according to the defined script.
    pub fn build(&mut self) -> Mock {
        let (mock, _) = self.build_with_handle();
        mock
    }

    /// Build a `Mock` value paired with a handle
    pub fn build_with_handle(&mut self) -> (Mock, Handle) {
        let (inner, handle) = Inner::new(self.actions.clone());

        let mock = Mock { inner };

        (mock, handle)
    }
}



impl Handle {
    /// Sequence a `read` operation.
    ///
    /// The next operation in the mock's script will be to expect a `read` call
    /// and return `buf`.
    pub fn read(&mut self, buf: &[u8]) -> &mut Self {
        self.tx.send(Action::Read(buf.into())).unwrap();
        self
    }

    /// Sequence a `read` operation error.
    ///
    /// The next operation in the mock's script will be to expect a `read` call
    /// and return `error`.
    pub fn read_error(&mut self, error: io::Error) -> &mut Self {
        let error = Some(error.into());
        self.tx.send(Action::ReadError(error)).unwrap();
        self
    }

    /// Sequence a `write` operation.
    ///
    /// The next operation in the mock's script will be to expect a `write`
    /// call.
    pub fn write(&mut self, buf: &[u8]) -> &mut Self {
        self.tx.send(Action::Write(buf.into())).unwrap();
        self
    }
    /// Sequence a `write` operation.
    ///
    /// The next operation in the mock's script will be to expect a `write`
    /// call.
    pub fn read_stream(&mut self, string: String) -> &mut Self {
        self.tx.send(Action::Next(string)).unwrap();
        self
    }

    /// Sequence a `write` operation error.
    ///
    /// The next operation in the mock's script will be to expect a `write`
    /// call error.
    pub fn write_error(&mut self, error: io::Error) -> &mut Self {
        let error = Some(error.into());
        self.tx.send(Action::WriteError(error)).unwrap();
        self
    }
}

impl Inner {
    fn new(actions: VecDeque<Action>) -> (Inner, Handle) {
        let (tx, rx) = mpsc::unbounded_channel();

        let rx = UnboundedReceiverStream::new(rx);

        let inner = Inner {
            actions,
            sleep: None,
            read_wait: None,
            rx,
            waiting: None,
        };

        let handle = Handle { tx };

        (inner, handle)
    }

    fn poll_action(&mut self, cx: &mut task::Context<'_>) -> Poll<Option<Action>> {
        Pin::new(&mut self.rx).poll_next(cx)
    }

    fn action(&mut self) -> Option<&mut Action> {
        loop {
            if self.actions.is_empty() {
                return None;
            }

            match self.actions[0] {
                Action::Read(ref mut data) => {
                    if !data.is_empty() {
                        break;
                    }
                }
                Action::Write(ref mut data) => {
                    if !data.is_empty() {
                        break;
                    }
                }
                Action::Next(ref mut data) => {
                    if !data.is_empty() {
                        break;
                    }
                }
                Action::Wait(ref mut dur) => {
                    if let Some(until) = self.waiting {
                        let now = Instant::now();

                        if now < until {
                            break;
                        } else {
                            self.waiting = None;
                        }
                    } else {
                        self.waiting = Some(Instant::now() + *dur);
                        break;
                    }
                }
                Action::ReadError(ref mut error) | Action::WriteError(ref mut error) => {
                    if error.is_some() {
                        break;
                    }
                }
            }

            let _action = self.actions.pop_front();
        }

        self.actions.front_mut()
    }
}

// ===== impl Inner =====


impl Stream for Mock {
    type Item = Vec<u8>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {

        loop {
            if let Some(ref mut sleep) = self.inner.sleep {
                ready!(Pin::new(sleep).poll(_cx));
            }


            // If a sleep is set, it has already fired
            self.inner.sleep = None;
 
            // match ready!
            match ready!(self.inner.poll_action(_cx)) {
                Some(Action::Read(data)) => {
                    return Poll::Ready(Some(data));
                }
                Some(Action::Write(data)) => {
                    return Poll::Ready(Some(data));
                }
                Some(Action::Next(data)) => {
                    return Poll::Ready(Some(data.as_bytes().to_vec()));
                }
                Some(Action::Wait(dur)) => {
                    self.inner.sleep = Some(delay_for(dur));
                }
                Some(Action::ReadError(error)) => {
                    return Poll::Ready(None);
                }
                Some(Action::WriteError(error)) => {
                    return Poll::Ready(None);
                }
                Some(_) => {
                    continue;
                }
                None => {
                    return Poll::Ready(None);
                }

            }
        }

    }


}

/// Ensures that Mock isn't dropped with data "inside".
impl Drop for Mock {
    fn drop(&mut self) {
        // Avoid double panicking, since makes debugging much harder.
        if std::thread::panicking() {
            return;
        }

        self.inner.actions.iter().for_each(|a| match a {
            Action::Read(data) => assert!(data.is_empty(), "There is still data left to read."),
            Action::Write(data) => assert!(data.is_empty(), "There is still data left to write."),
            _ => (),
        })
    }
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Inner {{...}}")
    }
}
