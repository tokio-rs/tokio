#![cfg_attr(not(feature = "sync"), allow(dead_code, unreachable_pub))]

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
//! value. The **latest** value stored in the channel is accessed with
//! [`Receiver::borrow()`]. Awaiting [`Receiver::changed()`] waits for a new
//! value to sent by the [`Sender`] half.
//!
//! # Examples
//!
//! ```
//! use tokio::sync::watch;
//!
//! # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
//!     let (tx, mut rx) = watch::channel("hello");
//!
//!     tokio::spawn(async move {
//!         while rx.changed().await.is_ok() {
//!             println!("received = {:?}", *rx.borrow());
//!         }
//!     });
//!
//!     tx.send("world")?;
//! # Ok(())
//! # }
//! ```
//!
//! # Closing
//!
//! [`Sender::is_closed`] and [`Sender::closed`] allow the producer to detect
//! when all [`Receiver`] handles have been dropped. This indicates that there
//! is no further interest in the values being produced and work can be stopped.
//!
//! # Thread safety
//!
//! Both [`Sender`] and [`Receiver`] are thread safe. They can be moved to other
//! threads and can be used in a concurrent environment. Clones of [`Receiver`]
//! handles may be moved to separate threads and also used concurrently.
//!
//! [`Sender`]: crate::sync::watch::Sender
//! [`Receiver`]: crate::sync::watch::Receiver
//! [`Receiver::changed()`]: crate::sync::watch::Receiver::changed
//! [`Receiver::borrow()`]: crate::sync::watch::Receiver::borrow
//! [`channel`]: crate::sync::watch::channel
//! [`Sender::is_closed`]: crate::sync::watch::Sender::is_closed
//! [`Sender::closed`]: crate::sync::watch::Sender::closed

use crate::sync::notify::Notify;

use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::atomic::Ordering::Relaxed;
use crate::loom::sync::{Arc, RwLock, RwLockReadGuard};
use std::ops;

/// Receives values from the associated [`Sender`](struct@Sender).
///
/// Instances are created by the [`channel`](fn@channel) function.
///
/// To turn this receiver into a `Stream`, you can use the [`WatchStream`]
/// wrapper.
///
/// [`WatchStream`]: https://docs.rs/tokio-stream/0.1/tokio_stream/wrappers/struct.WatchStream.html
#[derive(Debug)]
pub struct Receiver<T> {
    /// Pointer to the shared state
    shared: Arc<Shared<T>>,

    /// Last observed version
    version: Version,
}

/// Sends values to the associated [`Receiver`](struct@Receiver).
///
/// Instances are created by the [`channel`](fn@channel) function.
#[derive(Debug)]
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
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

#[derive(Debug)]
struct Shared<T> {
    /// The most recent value
    value: RwLock<T>,

    /// The current version
    ///
    /// The lowest bit represents a "closed" state. The rest of the bits
    /// represent the current version.
    state: AtomicState,

    /// Tracks the number of `Receiver` instances
    ref_count_rx: AtomicUsize,

    /// Notifies waiting receivers that the value changed.
    notify_rx: Notify,

    /// Notifies any task listening for `Receiver` dropped events
    notify_tx: Notify,
}

pub mod error {
    //! Watch error types

    use std::fmt;

    /// Error produced when sending a value fails.
    #[derive(Debug)]
    pub struct SendError<T>(pub T);

    // ===== impl SendError =====

    impl<T: fmt::Debug> fmt::Display for SendError<T> {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "channel closed")
        }
    }

    impl<T: fmt::Debug> std::error::Error for SendError<T> {}

    /// Error produced when receiving a change notification.
    #[derive(Debug)]
    pub struct RecvError(pub(super) ());

    // ===== impl RecvError =====

    impl fmt::Display for RecvError {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "channel closed")
        }
    }

    impl std::error::Error for RecvError {}
}

use self::state::{AtomicState, Version};
mod state {
    use crate::loom::sync::atomic::AtomicUsize;
    use crate::loom::sync::atomic::Ordering::SeqCst;

    const CLOSED: usize = 1;

    /// The version part of the state. The lowest bit is always zero.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub(super) struct Version(usize);

    /// Snapshot of the state. The first bit is used as the CLOSED bit.
    /// The remaining bits are used as the version.
    ///
    /// The CLOSED bit tracks whether the Sender has been dropped. Dropping all
    /// receivers does not set it.
    #[derive(Copy, Clone, Debug)]
    pub(super) struct StateSnapshot(usize);

    /// The state stored in an atomic integer.
    #[derive(Debug)]
    pub(super) struct AtomicState(AtomicUsize);

    impl Version {
        /// Get the initial version when creating the channel.
        pub(super) fn initial() -> Self {
            Version(0)
        }
    }

    impl StateSnapshot {
        /// Extract the version from the state.
        pub(super) fn version(self) -> Version {
            Version(self.0 & !CLOSED)
        }

        /// Is the closed bit set?
        pub(super) fn is_closed(self) -> bool {
            (self.0 & CLOSED) == CLOSED
        }
    }

    impl AtomicState {
        /// Create a new `AtomicState` that is not closed and which has the
        /// version set to `Version::initial()`.
        pub(super) fn new() -> Self {
            AtomicState(AtomicUsize::new(0))
        }

        /// Load the current value of the state.
        pub(super) fn load(&self) -> StateSnapshot {
            StateSnapshot(self.0.load(SeqCst))
        }

        /// Increment the version counter.
        pub(super) fn increment_version(&self) {
            // Increment by two to avoid touching the CLOSED bit.
            self.0.fetch_add(2, SeqCst);
        }

        /// Set the closed bit in the state.
        pub(super) fn set_closed(&self) {
            self.0.fetch_or(CLOSED, SeqCst);
        }
    }
}

/// Creates a new watch channel, returning the "send" and "receive" handles.
///
/// All values sent by [`Sender`] will become visible to the [`Receiver`] handles.
/// Only the last value sent is made available to the [`Receiver`] half. All
/// intermediate values are dropped.
///
/// # Examples
///
/// ```
/// use tokio::sync::watch;
///
/// # async fn dox() -> Result<(), Box<dyn std::error::Error>> {
///     let (tx, mut rx) = watch::channel("hello");
///
///     tokio::spawn(async move {
///         while rx.changed().await.is_ok() {
///             println!("received = {:?}", *rx.borrow());
///         }
///     });
///
///     tx.send("world")?;
/// # Ok(())
/// # }
/// ```
///
/// [`Sender`]: struct@Sender
/// [`Receiver`]: struct@Receiver
pub fn channel<T>(init: T) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        value: RwLock::new(init),
        state: AtomicState::new(),
        ref_count_rx: AtomicUsize::new(1),
        notify_rx: Notify::new(),
        notify_tx: Notify::new(),
    });

    let tx = Sender {
        shared: shared.clone(),
    };

    let rx = Receiver {
        shared,
        version: Version::initial(),
    };

    (tx, rx)
}

impl<T> Receiver<T> {
    fn from_shared(version: Version, shared: Arc<Shared<T>>) -> Self {
        // No synchronization necessary as this is only used as a counter and
        // not memory access.
        shared.ref_count_rx.fetch_add(1, Relaxed);

        Self { shared, version }
    }

    /// Returns a reference to the most recently sent value.
    ///
    /// This method does not mark the returned value as seen, so future calls to
    /// [`changed`] may return immediately even if you have already seen the
    /// value with a call to `borrow`.
    ///
    /// Outstanding borrows hold a read lock. This means that long lived borrows
    /// could cause the send half to block. It is recommended to keep the borrow
    /// as short lived as possible.
    ///
    /// [`changed`]: Receiver::changed
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::watch;
    ///
    /// let (_, rx) = watch::channel("hello");
    /// assert_eq!(*rx.borrow(), "hello");
    /// ```
    pub fn borrow(&self) -> Ref<'_, T> {
        let inner = self.shared.value.read().unwrap();
        Ref { inner }
    }

    /// Returns a reference to the most recently sent value and mark that value
    /// as seen.
    ///
    /// This method marks the value as seen, so [`changed`] will not return
    /// immediately if the newest value is one previously returned by
    /// `borrow_and_update`.
    ///
    /// Outstanding borrows hold a read lock. This means that long lived borrows
    /// could cause the send half to block. It is recommended to keep the borrow
    /// as short lived as possible.
    ///
    /// [`changed`]: Receiver::changed
    pub fn borrow_and_update(&mut self) -> Ref<'_, T> {
        let inner = self.shared.value.read().unwrap();
        self.version = self.shared.state.load().version();
        Ref { inner }
    }

    /// Wait for a change notification, then mark the newest value as seen.
    ///
    /// If the newest value in the channel has not yet been marked seen when
    /// this method is called, the method marks that value seen and returns
    /// immediately. If the newest value has already been marked seen, then the
    /// method sleeps until a new message is sent by the [`Sender`] connected to
    /// this `Receiver`, or until the [`Sender`] is dropped.
    ///
    /// This method returns an error if and only if the [`Sender`] is dropped.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe. If you use it as the event in a
    /// [`tokio::select!`](crate::select) statement and some other branch
    /// completes first, then it is guaranteed that no values have been marked
    /// seen by this call to `changed`.
    ///
    /// [`Sender`]: struct@Sender
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::watch;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = watch::channel("hello");
    ///
    ///     tokio::spawn(async move {
    ///         tx.send("goodbye").unwrap();
    ///     });
    ///
    ///     assert!(rx.changed().await.is_ok());
    ///     assert_eq!(*rx.borrow(), "goodbye");
    ///
    ///     // The `tx` handle has been dropped
    ///     assert!(rx.changed().await.is_err());
    /// }
    /// ```
    pub async fn changed(&mut self) -> Result<(), error::RecvError> {
        loop {
            // In order to avoid a race condition, we first request a notification,
            // **then** check the current value's version. If a new version exists,
            // the notification request is dropped.
            let notified = self.shared.notify_rx.notified();

            if let Some(ret) = maybe_changed(&self.shared, &mut self.version) {
                return ret;
            }

            notified.await;
            // loop around again in case the wake-up was spurious
        }
    }

    cfg_process_driver! {
        pub(crate) fn try_has_changed(&mut self) -> Option<Result<(), error::RecvError>> {
            maybe_changed(&self.shared, &mut self.version)
        }
    }
}

fn maybe_changed<T>(
    shared: &Shared<T>,
    version: &mut Version,
) -> Option<Result<(), error::RecvError>> {
    // Load the version from the state
    let state = shared.state.load();
    let new_version = state.version();

    if *version != new_version {
        // Observe the new version and return
        *version = new_version;
        return Some(Ok(()));
    }

    if state.is_closed() {
        // All receivers have dropped.
        return Some(Err(error::RecvError(())));
    }

    None
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        let version = self.version;
        let shared = self.shared.clone();

        Self::from_shared(version, shared)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        // No synchronization necessary as this is only used as a counter and
        // not memory access.
        if 1 == self.shared.ref_count_rx.fetch_sub(1, Relaxed) {
            // This is the last `Receiver` handle, tasks waiting on `Sender::closed()`
            self.shared.notify_tx.notify_waiters();
        }
    }
}

impl<T> Sender<T> {
    /// Sends a new value via the channel, notifying all receivers.
    ///
    /// This method fails if the channel has been closed, which happens when
    /// every receiver has been dropped.
    pub fn send(&self, value: T) -> Result<(), error::SendError<T>> {
        // This is pretty much only useful as a hint anyway, so synchronization isn't critical.
        if 0 == self.receiver_count() {
            return Err(error::SendError(value));
        }

        {
            // Acquire the write lock and update the value.
            let mut lock = self.shared.value.write().unwrap();
            *lock = value;

            self.shared.state.increment_version();

            // Release the write lock.
            //
            // Incrementing the version counter while holding the lock ensures
            // that receivers are able to figure out the version number of the
            // value they are currently looking at.
            drop(lock);
        }

        // Notify all watchers
        self.shared.notify_rx.notify_waiters();

        Ok(())
    }

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
    /// let (tx, _) = watch::channel("hello");
    /// assert_eq!(*tx.borrow(), "hello");
    /// ```
    pub fn borrow(&self) -> Ref<'_, T> {
        let inner = self.shared.value.read().unwrap();
        Ref { inner }
    }

    /// Checks if the channel has been closed. This happens when all receivers
    /// have dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// let (tx, rx) = tokio::sync::watch::channel(());
    /// assert!(!tx.is_closed());
    ///
    /// drop(rx);
    /// assert!(tx.is_closed());
    /// ```
    pub fn is_closed(&self) -> bool {
        self.receiver_count() == 0
    }

    /// Completes when all receivers have dropped.
    ///
    /// This allows the producer to get notified when interest in the produced
    /// values is canceled and immediately stop doing work.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe. Once the channel is closed, it stays closed
    /// forever and all future calls to `closed` will return immediately.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::watch;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, rx) = watch::channel("hello");
    ///
    ///     tokio::spawn(async move {
    ///         // use `rx`
    ///         drop(rx);
    ///     });
    ///
    ///     // Waits for `rx` to drop
    ///     tx.closed().await;
    ///     println!("the `rx` handles dropped")
    /// }
    /// ```
    pub async fn closed(&self) {
        while self.receiver_count() > 0 {
            let notified = self.shared.notify_tx.notified();

            if self.receiver_count() == 0 {
                return;
            }

            notified.await;
            // The channel could have been reopened in the meantime by calling
            // `subscribe`, so we loop again.
        }
    }

    /// Creates a new [`Receiver`] connected to this `Sender`.
    ///
    /// All messages sent before this call to `subscribe` are initially marked
    /// as seen by the new `Receiver`.
    ///
    /// This method can be called even if there are no other receivers. In this
    /// case, the channel is reopened.
    ///
    /// # Examples
    ///
    /// The new channel will receive messages sent on this `Sender`.
    ///
    /// ```
    /// use tokio::sync::watch;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, _rx) = watch::channel(0u64);
    ///
    ///     tx.send(5).unwrap();
    ///
    ///     let rx = tx.subscribe();
    ///     assert_eq!(5, *rx.borrow());
    ///
    ///     tx.send(10).unwrap();
    ///     assert_eq!(10, *rx.borrow());
    /// }
    /// ```
    ///
    /// The most recent message is considered seen by the channel, so this test
    /// is guaranteed to pass.
    ///
    /// ```
    /// use tokio::sync::watch;
    /// use tokio::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, _rx) = watch::channel(0u64);
    ///     tx.send(5).unwrap();
    ///     let mut rx = tx.subscribe();
    ///
    ///     tokio::spawn(async move {
    ///         // by spawning and sleeping, the message is sent after `main`
    ///         // hits the call to `changed`.
    ///         # if false {
    ///         tokio::time::sleep(Duration::from_millis(10)).await;
    ///         # }
    ///         tx.send(100).unwrap();
    ///     });
    ///
    ///     rx.changed().await.unwrap();
    ///     assert_eq!(100, *rx.borrow());
    /// }
    /// ```
    pub fn subscribe(&self) -> Receiver<T> {
        let shared = self.shared.clone();
        let version = shared.state.load().version();

        // The CLOSED bit in the state tracks only whether the sender is
        // dropped, so we do not need to unset it if this reopens the channel.
        Receiver::from_shared(version, shared)
    }

    /// Returns the number of receivers that currently exist
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::watch;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, rx1) = watch::channel("hello");
    ///
    ///     assert_eq!(1, tx.receiver_count());
    ///
    ///     let mut _rx2 = rx1.clone();
    ///
    ///     assert_eq!(2, tx.receiver_count());
    /// }
    /// ```
    pub fn receiver_count(&self) -> usize {
        self.shared.ref_count_rx.load(Relaxed)
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.shared.state.set_closed();
        self.shared.notify_rx.notify_waiters();
    }
}

// ===== impl Ref =====

impl<T> ops::Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

#[cfg(all(test, loom))]
mod tests {
    use futures::future::FutureExt;
    use loom::thread;

    // test for https://github.com/tokio-rs/tokio/issues/3168
    #[test]
    fn watch_spurious_wakeup() {
        loom::model(|| {
            let (send, mut recv) = crate::sync::watch::channel(0i32);

            send.send(1).unwrap();

            let send_thread = thread::spawn(move || {
                send.send(2).unwrap();
                send
            });

            recv.changed().now_or_never();

            let send = send_thread.join().unwrap();
            let recv_thread = thread::spawn(move || {
                recv.changed().now_or_never();
                recv.changed().now_or_never();
                recv
            });

            send.send(3).unwrap();

            let mut recv = recv_thread.join().unwrap();
            let send_thread = thread::spawn(move || {
                send.send(2).unwrap();
            });

            recv.changed().now_or_never();

            send_thread.join().unwrap();
        });
    }

    #[test]
    fn watch_borrow() {
        loom::model(|| {
            let (send, mut recv) = crate::sync::watch::channel(0i32);

            assert!(send.borrow().eq(&0));
            assert!(recv.borrow().eq(&0));

            send.send(1).unwrap();
            assert!(send.borrow().eq(&1));

            let send_thread = thread::spawn(move || {
                send.send(2).unwrap();
                send
            });

            recv.changed().now_or_never();

            let send = send_thread.join().unwrap();
            let recv_thread = thread::spawn(move || {
                recv.changed().now_or_never();
                recv.changed().now_or_never();
                recv
            });

            send.send(3).unwrap();

            let recv = recv_thread.join().unwrap();
            assert!(recv.borrow().eq(&3));
            assert!(send.borrow().eq(&3));

            send.send(2).unwrap();

            thread::spawn(move || {
                assert!(recv.borrow().eq(&2));
            });
            assert!(send.borrow().eq(&2));
        });
    }
}
