//! A single-producer, multi-consumer channel that only retains the *last* sent
//! value.
//!
//! This channel is useful for watching for changes to a value from multiple
//! points in the code base, for example, changes to configuration values.
//!
//! # Usage
//!
//! [`channel`] returns a [`Sender`] / [`Receiver`] pair. These are the producer
//! and sender halves of the channel. The channel is created with an initial
//! value. `Receiver::poll` will always be ready upon creation and will yield
//! either this initial value or the latest value that has been sent by
//! `Sender`.
//!
//! Calls to [`Receiver::poll`] and [`Receiver::poll_ref`] will always yield
//! the latest value.
//!
//! # Examples
//!
//! ```
//! use tokio::prelude::*;
//! use tokio::sync::watch;
//!
//! # tokio::run(futures::future::lazy(|| {
//! let (mut tx, rx) = watch::channel("hello");
//!
//! tokio::spawn(rx.for_each(|value| {
//!     println!("received = {:?}", value);
//!     Ok(())
//! }).map_err(|_| ()));
//!
//! tx.broadcast("world").unwrap();
//! # Ok(())
//! # }));
//! ```
//!
//! # Closing
//!
//! [`Sender::poll_close`] allows the producer to detect when all [`Sender`]
//! handles have been dropped. This indicates that there is no further interest
//! in the values being produced and work can be stopped.
//!
//! # Thread safety
//!
//! Both [`Sender`] and [`Receiver`] are thread safe. They can be moved to other
//! threads and can be used in a concurrent environment. Clones of [`Receiver`]
//! handles may be moved to separate threads and also used concurrently.
//!
//! [`Sender`]: struct.Sender.html
//! [`Receiver`]: struct.Receiver.html
//! [`channel`]: fn.channel.html
//! [`Sender::poll_close`]: struct.Sender.html#method.poll_close
//! [`Receiver::poll`]: struct.Receiver.html#method.poll
//! [`Receiver::poll_ref`]: struct.Receiver.html#method.poll_ref

use crate::task::AtomicWaker;

use core::task::Poll::{Pending, Ready};
use core::task::{Context, Poll};
use fnv::FnvHashMap;
use std::ops;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, Weak};

#[cfg(feature = "async-traits")]
use std::pin::Pin;

/// Receives values from the associated `Sender`.
///
/// Instances are created by the [`channel`](fn.channel.html) function.
#[derive(Debug)]
pub struct Receiver<T> {
    /// Pointer to the shared state
    shared: Arc<Shared<T>>,

    /// Pointer to the watcher's internal state
    inner: Arc<WatchInner>,

    /// Watcher ID.
    id: u64,

    /// Last observed version
    ver: usize,
}

/// Sends values to the associated `Receiver`.
///
/// Instances are created by the [`channel`](fn.channel.html) function.
#[derive(Debug)]
pub struct Sender<T> {
    shared: Weak<Shared<T>>,
}

/// Returns a reference to the inner value
///
/// Outstanding borrows hold a read lock on the inner value. This means that
/// long lived borrows could cause the produce half to block. It is recommended
/// to keep the borrow as short lived as possible.
#[derive(Debug)]
pub struct Ref<'a, T> {
    inner: RwLockReadGuard<'a, T>,
}

pub mod error {
    //! Watch error types

    use std::fmt;

    /// Error produced when sending a value fails.
    #[derive(Debug)]
    pub struct SendError<T> {
        pub(crate) inner: T,
    }

    // ===== impl SendError =====

    impl<T: fmt::Debug> fmt::Display for SendError<T> {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "channel closed")
        }
    }

    impl<T: fmt::Debug> ::std::error::Error for SendError<T> {}
}

#[derive(Debug)]
struct Shared<T> {
    /// The most recent value
    value: RwLock<T>,

    /// The current version
    ///
    /// The lowest bit represents a "closed" state. The rest of the bits
    /// represent the current version.
    version: AtomicUsize,

    /// All watchers
    watchers: Mutex<Watchers>,

    /// Task to notify when all watchers drop
    cancel: AtomicWaker,
}

#[derive(Debug)]
struct Watchers {
    next_id: u64,
    watchers: FnvHashMap<u64, Arc<WatchInner>>,
}

#[derive(Debug)]
struct WatchInner {
    waker: AtomicWaker,
}

const CLOSED: usize = 1;

/// Create a new watch channel, returning the "send" and "receive" handles.
///
/// All values sent by `Sender` will become visible to the `Receiver` handles.
/// Only the last value sent is made available to the `Receiver` half. All
/// intermediate values are dropped.
///
/// # Examples
///
/// ```
/// use tokio::prelude::*;
/// use tokio::sync::watch;
///
/// # tokio::run(futures::future::lazy(|| {
/// let (mut tx, rx) = watch::channel("hello");
///
/// tokio::spawn(rx.for_each(|value| {
///     println!("received = {:?}", value);
///     Ok(())
/// }).map_err(|_| ()));
///
/// tx.broadcast("world").unwrap();
/// # Ok(())
/// # }));
/// ```
pub fn channel<T>(init: T) -> (Sender<T>, Receiver<T>) {
    const INIT_ID: u64 = 0;

    let inner = Arc::new(WatchInner::new());

    // Insert the watcher
    let mut watchers = FnvHashMap::with_capacity_and_hasher(0, Default::default());
    watchers.insert(INIT_ID, inner.clone());

    let shared = Arc::new(Shared {
        value: RwLock::new(init),
        version: AtomicUsize::new(2),
        watchers: Mutex::new(Watchers {
            next_id: INIT_ID + 1,
            watchers,
        }),
        cancel: AtomicWaker::new(),
    });

    let tx = Sender {
        shared: Arc::downgrade(&shared),
    };

    let rx = Receiver {
        shared,
        inner,
        id: INIT_ID,
        ver: 0,
    };

    (tx, rx)
}

impl<T> Receiver<T> {
    /// Returns a reference to the most recently sent value
    ///
    /// Outstanding borrows hold a read lock. This means that long lived borrows
    /// could cause the send half to block. It is recommended to keep the borrow
    /// as short lived as possible.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::watch;
    ///
    /// let (_, rx) = watch::channel("hello");
    /// assert_eq!(*rx.get_ref(), "hello");
    /// ```
    pub fn get_ref(&self) -> Ref<'_, T> {
        let inner = self.shared.value.read().unwrap();
        Ref { inner }
    }

    /// Attempts to receive the latest value sent via the channel.
    ///
    /// If a new, unobserved, value has been sent, a reference to it is
    /// returned. If no new value has been sent, then `Pending` is returned and
    /// the current task is notified once a new value is sent.
    ///
    /// Only the **most recent** value is returned. If the receiver is falling
    /// behind the sender, intermediate values are dropped.
    pub fn poll_ref(&mut self, cx: &mut Context<'_>) -> Poll<Option<Ref<'_, T>>> {
        // Make sure the task is up to date
        self.inner.waker.register_by_ref(cx.waker());

        let state = self.shared.version.load(SeqCst);
        let version = state & !CLOSED;

        if version != self.ver {
            // Track the latest version
            self.ver = version;

            let inner = self.shared.value.read().unwrap();

            return Ready(Some(Ref { inner }));
        }

        if CLOSED == state & CLOSED {
            // The `Store` handle has been dropped.
            return Ready(None);
        }

        Pending
    }
}

impl<T: Clone> Receiver<T> {
    /// Attempts to clone the latest value sent via the channel.
    ///
    /// This is equivalent to calling `Clone` on the value returned by `poll_ref`.
    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let item = ready!(self.poll_ref(cx));
        Ready(item.map(|v_ref| v_ref.clone()))
    }
}

#[cfg(feature = "async-traits")]
impl<T: Clone> futures_core::Stream for Receiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let item = ready!(self.poll_ref(cx));
        Ready(item.map(|v_ref| v_ref.clone()))
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        let inner = Arc::new(WatchInner::new());
        let shared = self.shared.clone();

        let id = {
            let mut watchers = shared.watchers.lock().unwrap();
            let id = watchers.next_id;

            watchers.next_id += 1;
            watchers.watchers.insert(id, inner.clone());

            id
        };

        let ver = self.ver;

        Receiver {
            shared: shared,
            inner,
            id,
            ver,
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut watchers = self.shared.watchers.lock().unwrap();
        watchers.watchers.remove(&self.id);
    }
}

impl WatchInner {
    fn new() -> Self {
        WatchInner {
            waker: AtomicWaker::new(),
        }
    }
}

impl<T> Sender<T> {
    /// Broadcast a new value via the channel, notifying all receivers.
    pub fn broadcast(&self, value: T) -> Result<(), error::SendError<T>> {
        let shared = match self.shared.upgrade() {
            Some(shared) => shared,
            // All `Watch` handles have been canceled
            None => return Err(error::SendError { inner: value }),
        };

        // Replace the value
        {
            let mut lock = shared.value.write().unwrap();
            *lock = value;
        }

        // Update the version. 2 is used so that the CLOSED bit is not set.
        shared.version.fetch_add(2, SeqCst);

        // Notify all watchers
        notify_all(&*shared);

        // Return the old value
        Ok(())
    }

    /// Returns `Ready` when all receivers have dropped.
    ///
    /// This allows the producer to get notified when interest in the produced
    /// values is canceled and immediately stop doing work.
    pub fn poll_close(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self.shared.upgrade() {
            Some(shared) => {
                shared.cancel.register_by_ref(cx.waker());
                Pending
            }
            None => Ready(()),
        }
    }
}

#[cfg(feature = "async-traits")]
impl<T> async_sink::Sink<T> for Sender<T> {
    type Error = error::SendError<T>;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        let _ = self.as_ref().get_ref().broadcast(item)?;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ready(Ok(()))
    }
}

/// Notify all watchers of a change
fn notify_all<T>(shared: &Shared<T>) {
    let watchers = shared.watchers.lock().unwrap();

    for watcher in watchers.watchers.values() {
        // Notify the task
        watcher.waker.wake();
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.version.fetch_or(CLOSED, SeqCst);
            notify_all(&*shared);
        }
    }
}

// ===== impl Ref =====

impl<'a, T: 'a> ops::Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

// ===== impl Shared =====

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        self.cancel.wake();
    }
}
