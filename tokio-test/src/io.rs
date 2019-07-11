//! A mock type implementing [`AsyncRead`] and [`AsyncWrite`].
//!
//!
//! # Overview
//!
//! Provides a type that implements [`AsyncRead`] + [`AsyncWrite`] that can be configured
//! to handle an arbitrary sequence of read and write operations. This is useful
//! for writing unit tests for networking services as using an actual network
//! type is fairly non deterministic.
//!
//! # Usage
//!
//! Attempting to write data that the mock isn't expected will result in a
//! panic.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll, Waker};
use std::time::{Duration, Instant};
use std::{cmp, io};

use tokio_io::{AsyncRead, AsyncWrite};
use tokio_sync::mpsc;
use tokio_timer::{clock, timer, Delay};

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
pub struct Builder {
    // Sequence of actions for the Mock to take
    actions: VecDeque<Action>,
}

#[derive(Debug, Clone)]
enum Action {
    Read(Vec<u8>),
    Write(Vec<u8>),
    Wait(Duration),
}

#[derive(Debug)]
struct Inner {
    actions: VecDeque<Action>,
    waiting: Option<Instant>,

    timer_handle: timer::Handle,
    sleep: Option<Delay>,
    read_wait: Option<Waker>,
    rx: mpsc::UnboundedReceiver<Action>,
}

impl Builder {
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

    /// Sequence a `write` operation.
    ///
    /// The next operation in the mock's script will be to expect a `write`
    /// call.
    pub fn write(&mut self, buf: &[u8]) -> &mut Self {
        self.actions.push_back(Action::Write(buf.into()));
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
        self.tx.try_send(Action::Read(buf.into())).unwrap();
        self
    }

    /// Sequence a `write` operation.
    ///
    /// The next operation in the mock's script will be to expect a `write`
    /// call.
    pub fn write(&mut self, buf: &[u8]) -> &mut Self {
        self.tx.try_send(Action::Write(buf.into())).unwrap();
        self
    }
}

impl Inner {
    fn new(actions: VecDeque<Action>) -> (Inner, Handle) {
        let (tx, rx) = mpsc::unbounded_channel();

        let inner = Inner {
            actions,
            timer_handle: timer::Handle::default(),
            sleep: None,
            read_wait: None,
            rx,
            waiting: None,
        };

        let handle = Handle { tx };

        (inner, handle)
    }

    fn poll_action(&mut self, cx: &mut task::Context<'_>) -> Poll<Option<Action>> {
        self.rx.poll_recv(cx)
    }

    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        match self.action() {
            Some(&mut Action::Read(ref mut data)) => {
                // Figure out how much to copy
                let n = cmp::min(dst.len(), data.len());

                // Copy the data into the `dst` slice
                (&mut dst[..n]).copy_from_slice(&data[..n]);

                // Drain the data from the source
                data.drain(..n);

                // Return the number of bytes read
                Ok(n)
            }
            Some(_) => {
                // Either waiting or expecting a write
                Err(io::ErrorKind::WouldBlock.into())
            }
            None => Ok(0),
        }
    }

    fn write(&mut self, mut src: &[u8]) -> io::Result<usize> {
        let mut ret = 0;

        if self.actions.is_empty() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }

        match self.action() {
            Some(&mut Action::Wait(..)) => {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            _ => {}
        }

        for i in 0..self.actions.len() {
            match self.actions[i] {
                Action::Write(ref mut expect) => {
                    let n = cmp::min(src.len(), expect.len());

                    assert_eq!(&src[..n], &expect[..n]);

                    // Drop data that was matched
                    expect.drain(..n);
                    src = &src[n..];

                    ret += n;

                    if src.is_empty() {
                        return Ok(ret);
                    }
                }
                Action::Wait(..) => {
                    break;
                }
                _ => {}
            }

            // TODO: remove write
        }

        Ok(ret)
    }

    fn remaining_wait(&mut self) -> Option<Duration> {
        match self.action() {
            Some(&mut Action::Wait(dur)) => Some(dur),
            _ => None,
        }
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
                Action::Wait(ref mut dur) => {
                    if let Some(until) = self.waiting {
                        let now = Instant::now();

                        if now < until {
                            break;
                        }
                    } else {
                        self.waiting = Some(Instant::now() + *dur);
                        break;
                    }
                }
            }

            let _action = self.actions.pop_front();
        }

        self.actions.front_mut()
    }
}

/*
impl io::Read for Mock {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        if self.is_async() {
            tokio::async_read(self, dst)
        } else {
            self.sync_read(dst)
        }
    }
}

impl io::Write for Mock {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        if self.is_async() {
            tokio::async_write(self, src)
        } else {
            self.sync_write(src)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
*/

/*
use self::futures::{Future, Stream, Poll, Async};
use self::futures::sync::mpsc;
use self::futures::task::{self, Task};
use self::tokio_io::{AsyncRead, AsyncWrite};
use self::tokio_timer::{Timer, Sleep};

use std::io;
*/

// ===== impl Inner =====

impl Mock {
    fn maybe_wakeup_reader(&mut self) {
        match self.inner.action() {
            Some(&mut Action::Read(_)) | None => {
                if let Some(waker) = self.inner.read_wait.take() {
                    waker.wake();
                }
            }
            _ => {}
        }
    }
}

impl AsyncRead for Mock {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            if let Some(ref mut sleep) = self.inner.sleep {
                ready!(Pin::new(sleep).poll(cx));
            }

            // If a sleep is set, it has already fired
            self.inner.sleep = None;

            match self.inner.read(buf) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(rem) = self.inner.remaining_wait() {
                        let until = clock::now() + rem;
                        self.inner.sleep = Some(self.inner.timer_handle.delay(until));
                    } else {
                        self.inner.read_wait = Some(cx.waker().clone());
                        return Poll::Pending;
                    }
                }
                Ok(0) => {
                    // TODO: Extract
                    match ready!(self.inner.poll_action(cx)) {
                        Some(action) => {
                            self.inner.actions.push_back(action);
                            continue;
                        }
                        None => {
                            return Poll::Ready(Ok(0));
                        }
                    }
                }
                ret => return Poll::Ready(ret),
            }
        }
    }
}

impl AsyncWrite for Mock {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            if let Some(ref mut sleep) = self.inner.sleep {
                ready!(Pin::new(sleep).poll(cx));
            }

            // If a sleep is set, it has already fired
            self.inner.sleep = None;

            match self.inner.write(buf) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(rem) = self.inner.remaining_wait() {
                        let until = clock::now() + rem;
                        self.inner.sleep = Some(self.inner.timer_handle.delay(until));
                    } else {
                        panic!("unexpected WouldBlock");
                    }
                }
                Ok(0) => {
                    // TODO: Is this correct?
                    if !self.inner.actions.is_empty() {
                        return Poll::Pending;
                    }

                    // TODO: Extract
                    match ready!(self.inner.poll_action(cx)) {
                        Some(action) => {
                            self.inner.actions.push_back(action);
                            continue;
                        }
                        None => {
                            panic!("unexpected write");
                        }
                    }
                }
                ret => {
                    self.maybe_wakeup_reader();
                    return Poll::Ready(ret);
                }
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/*
/// Returns `true` if called from the context of a futures-rs Task
fn is_task_ctx() -> bool {
    use std::panic;

    // Save the existing panic hook
    let h = panic::take_hook();

    // Install a new one that does nothing
    panic::set_hook(Box::new(|_| {}));

    // Attempt to call the fn
    let r = panic::catch_unwind(|| task::current()).is_ok();

    // Re-install the old one
    panic::set_hook(h);

    // Return the result
    r
}
*/
