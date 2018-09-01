//! A one-shot, futures-aware channel

use lock::Lock;

use futures::{Future, Poll, Async};
use futures::task::{self, Task};

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::error::Error;
use std::fmt;

/// A future representing the completion of a computation happening elsewhere in
/// memory.
///
/// This is created by the `oneshot::channel` function.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
}

/// Represents the completion half of a oneshot through which the result of a
/// computation is signaled.
///
/// This is created by the `oneshot::channel` function.
#[derive(Debug)]
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
}

/// Internal state of the `Receiver`/`Sender` pair above. This is all used as
/// the internal synchronization between the two for send/recv operations.
#[derive(Debug)]
struct Inner<T> {
    /// Indicates whether this oneshot is complete yet. This is filled in both
    /// by `Sender::drop` and by `Receiver::drop`, and both sides interpret it
    /// appropriately.
    ///
    /// For `Receiver`, if this is `true`, then it's guaranteed that `data` is
    /// unlocked and ready to be inspected.
    ///
    /// For `Sender` if this is `true` then the oneshot has gone away and it
    /// can return ready from `poll_cancel`.
    complete: AtomicBool,

    /// The actual data being transferred as part of this `Receiver`. This is
    /// filled in by `Sender::complete` and read by `Receiver::poll`.
    ///
    /// Note that this is protected by `Lock`, but it is in theory safe to
    /// replace with an `UnsafeCell` as it's actually protected by `complete`
    /// above. I wouldn't recommend doing this, however, unless someone is
    /// supremely confident in the various atomic orderings here and there.
    data: Lock<Option<T>>,

    /// Field to store the task which is blocked in `Receiver::poll`.
    ///
    /// This is filled in when a oneshot is polled but not ready yet. Note that
    /// the `Lock` here, unlike in `data` above, is important to resolve races.
    /// Both the `Receiver` and the `Sender` halves understand that if they
    /// can't acquire the lock then some important interference is happening.
    rx_task: Lock<Option<Task>>,

    /// Like `rx_task` above, except for the task blocked in
    /// `Sender::poll_cancel`. Additionally, `Lock` cannot be `UnsafeCell`.
    tx_task: Lock<Option<Task>>,
}

/// Creates a new futures-aware, one-shot channel.
///
/// This function is similar to Rust's channels found in the standard library.
/// Two halves are returned, the first of which is a `Sender` handle, used to
/// signal the end of a computation and provide its value. The second half is a
/// `Receiver` which implements the `Future` trait, resolving to the value that
/// was given to the `Sender` handle.
///
/// Each half can be separately owned and sent across threads/tasks.
///
/// # Examples
///
/// ```
/// extern crate tokio_channel;
/// extern crate futures;
///
/// use tokio_channel::oneshot;
/// use futures::*;
/// use std::thread;
///
/// # fn main() {
/// let (p, c) = oneshot::channel::<i32>();
///
/// thread::spawn(|| {
///     c.map(|i| {
///         println!("got: {}", i);
///     }).wait();
/// });
///
/// p.send(3).unwrap();
/// # }
/// ```
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner::new());
    let receiver = Receiver {
        inner: inner.clone(),
    };
    let sender = Sender {
        inner: inner,
    };
    (sender, receiver)
}

impl<T> Inner<T> {
    fn new() -> Inner<T> {
        Inner {
            complete: AtomicBool::new(false),
            data: Lock::new(None),
            rx_task: Lock::new(None),
            tx_task: Lock::new(None),
        }
    }

    fn send(&self, t: T) -> Result<(), T> {
        if self.complete.load(SeqCst) {
            return Err(t)
        }

        // Note that this lock acquisition may fail if the receiver
        // is closed and sets the `complete` flag to true, whereupon
        // the receiver may call `poll()`.
        if let Some(mut slot) = self.data.try_lock() {
            assert!(slot.is_none());
            *slot = Some(t);
            drop(slot);

            // If the receiver called `close()` between the check at the
            // start of the function, and the lock being released, then
            // the receiver may not be around to receive it, so try to
            // pull it back out.
            if self.complete.load(SeqCst) {
                // If lock acquisition fails, then receiver is actually
                // receiving it, so we're good.
                if let Some(mut slot) = self.data.try_lock() {
                    if let Some(t) = slot.take() {
                        return Err(t);
                    }
                }
            }
            Ok(())
        } else {
            // Must have been closed
            Err(t)
        }
    }

    fn poll_cancel(&self) -> Poll<(), ()> {
        // Fast path up first, just read the flag and see if our other half is
        // gone. This flag is set both in our destructor and the oneshot
        // destructor, but our destructor hasn't run yet so if it's set then the
        // oneshot is gone.
        if self.complete.load(SeqCst) {
            return Ok(Async::Ready(()))
        }

        // If our other half is not gone then we need to park our current task
        // and move it into the `notify_cancel` slot to get notified when it's
        // actually gone.
        //
        // If `try_lock` fails, then the `Receiver` is in the process of using
        // it, so we can deduce that it's now in the process of going away and
        // hence we're canceled. If it succeeds then we just store our handle.
        //
        // Crucially we then check `oneshot_gone` *again* before we return.
        // While we were storing our handle inside `notify_cancel` the `Receiver`
        // may have been dropped. The first thing it does is set the flag, and
        // if it fails to acquire the lock it assumes that we'll see the flag
        // later on. So... we then try to see the flag later on!
        let handle = task::current();
        match self.tx_task.try_lock() {
            Some(mut p) => *p = Some(handle),
            None => return Ok(Async::Ready(())),
        }
        if self.complete.load(SeqCst) {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }

    fn is_canceled(&self) -> bool {
        self.complete.load(SeqCst)
    }

    fn drop_tx(&self) {
        // Flag that we're a completed `Sender` and try to wake up a receiver.
        // Whether or not we actually stored any data will get picked up and
        // translated to either an item or cancellation.
        //
        // Note that if we fail to acquire the `rx_task` lock then that means
        // we're in one of two situations:
        //
        // 1. The receiver is trying to block in `poll`
        // 2. The receiver is being dropped
        //
        // In the first case it'll check the `complete` flag after it's done
        // blocking to see if it succeeded. In the latter case we don't need to
        // wake up anyone anyway. So in both cases it's ok to ignore the `None`
        // case of `try_lock` and bail out.
        //
        // The first case crucially depends on `Lock` using `SeqCst` ordering
        // under the hood. If it instead used `Release` / `Acquire` ordering,
        // then it would not necessarily synchronize with `inner.complete`
        // and deadlock might be possible, as was observed in
        // https://github.com/rust-lang-nursery/futures-rs/pull/219.
        self.complete.store(true, SeqCst);
        if let Some(mut slot) = self.rx_task.try_lock() {
            if let Some(task) = slot.take() {
                drop(slot);
                task.notify();
            }
        }
    }

    fn close_rx(&self) {
        // Flag our completion and then attempt to wake up the sender if it's
        // blocked. See comments in `drop` below for more info
        self.complete.store(true, SeqCst);
        if let Some(mut handle) = self.tx_task.try_lock() {
            if let Some(task) = handle.take() {
                drop(handle);
                task.notify()
            }
        }
    }

    fn recv(&self) -> Poll<T, Canceled> {
        let mut done = false;

        // Check to see if some data has arrived. If it hasn't then we need to
        // block our task.
        //
        // Note that the acquisition of the `rx_task` lock might fail below, but
        // the only situation where this can happen is during `Sender::drop`
        // when we are indeed completed already. If that's happening then we
        // know we're completed so keep going.
        if self.complete.load(SeqCst) {
            done = true;
        } else {
            let task = task::current();
            match self.rx_task.try_lock() {
                Some(mut slot) => *slot = Some(task),
                None => done = true,
            }
        }

        // If we're `done` via one of the paths above, then look at the data and
        // figure out what the answer is. If, however, we stored `rx_task`
        // successfully above we need to check again if we're completed in case
        // a message was sent while `rx_task` was locked and couldn't notify us
        // otherwise.
        //
        // If we're not done, and we're not complete, though, then we've
        // successfully blocked our task and we return `NotReady`.
        if done || self.complete.load(SeqCst) {
            // If taking the lock fails, the sender will realise that the we're
            // `done` when it checks the `complete` flag on the way out, and will
            // treat the send as a failure.
            if let Some(mut slot) = self.data.try_lock() {
                if let Some(data) = slot.take() {
                    return Ok(data.into());
                }
            }
            Err(Canceled)
        } else {
            Ok(Async::NotReady)
        }
    }

    fn drop_rx(&self) {
        // Indicate to the `Sender` that we're done, so any future calls to
        // `poll_cancel` are weeded out.
        self.complete.store(true, SeqCst);

        // If we've blocked a task then there's no need for it to stick around,
        // so we need to drop it. If this lock acquisition fails, though, then
        // it's just because our `Sender` is trying to take the task, so we
        // let them take care of that.
        if let Some(mut slot) = self.rx_task.try_lock() {
            let task = slot.take();
            drop(slot);
            drop(task);
        }

        // Finally, if our `Sender` wants to get notified of us going away, it
        // would have stored something in `tx_task`. Here we try to peel that
        // out and unpark it.
        //
        // Note that the `try_lock` here may fail, but only if the `Sender` is
        // in the process of filling in the task. If that happens then we
        // already flagged `complete` and they'll pick that up above.
        if let Some(mut handle) = self.tx_task.try_lock() {
            if let Some(task) = handle.take() {
                drop(handle);
                task.notify()
            }
        }
    }
}

impl<T> Sender<T> {
    #[deprecated(note = "renamed to `send`", since = "0.1.11")]
    #[doc(hidden)]
    #[cfg(feature = "with-deprecated")]
    pub fn complete(self, t: T) {
        drop(self.send(t));
    }

    /// Completes this oneshot with a successful result.
    ///
    /// This function will consume `self` and indicate to the other end, the
    /// `Receiver`, that the value provided is the result of the computation this
    /// represents.
    ///
    /// If the value is successfully enqueued for the remote end to receive,
    /// then `Ok(())` is returned. If the receiving end was deallocated before
    /// this function was called, however, then `Err` is returned with the value
    /// provided.
    pub fn send(self, t: T) -> Result<(), T> {
        self.inner.send(t)
    }

    /// Polls this `Sender` half to detect whether the `Receiver` this has
    /// paired with has gone away.
    ///
    /// This function can be used to learn about when the `Receiver` (consumer)
    /// half has gone away and nothing will be able to receive a message sent
    /// from `send`.
    ///
    /// If `Ready` is returned then it means that the `Receiver` has disappeared
    /// and the result this `Sender` would otherwise produce should no longer
    /// be produced.
    ///
    /// If `NotReady` is returned then the `Receiver` is still alive and may be
    /// able to receive a message if sent. The current task, however, is
    /// scheduled to receive a notification if the corresponding `Receiver` goes
    /// away.
    ///
    /// # Panics
    ///
    /// Like `Future::poll`, this function will panic if it's not called from
    /// within the context of a task. In other words, this should only ever be
    /// called from inside another future.
    ///
    /// If you're calling this function from a context that does not have a
    /// task, then you can use the `is_canceled` API instead.
    pub fn poll_cancel(&mut self) -> Poll<(), ()> {
        self.inner.poll_cancel()
    }

    /// Tests to see whether this `Sender`'s corresponding `Receiver`
    /// has gone away.
    ///
    /// This function can be used to learn about when the `Receiver` (consumer)
    /// half has gone away and nothing will be able to receive a message sent
    /// from `send`.
    ///
    /// Note that this function is intended to *not* be used in the context of a
    /// future. If you're implementing a future you probably want to call the
    /// `poll_cancel` function which will block the current task if the
    /// cancellation hasn't happened yet. This can be useful when working on a
    /// non-futures related thread, though, which would otherwise panic if
    /// `poll_cancel` were called.
    pub fn is_canceled(&self) -> bool {
        self.inner.is_canceled()
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.inner.drop_tx()
    }
}

/// Error returned from a `Receiver<T>` whenever the corresponding `Sender<T>`
/// is dropped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Canceled;

impl fmt::Display for Canceled {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "oneshot canceled")
    }
}

impl Error for Canceled {
    fn description(&self) -> &str {
        "oneshot canceled"
    }
}

impl<T> Receiver<T> {
    /// Gracefully close this receiver, preventing sending any future messages.
    ///
    /// Any `send` operation which happens after this method returns is
    /// guaranteed to fail. Once this method is called the normal `poll` method
    /// can be used to determine whether a message was actually sent or not. If
    /// `Canceled` is returned from `poll` then no message was sent.
    pub fn close(&mut self) {
        self.inner.close_rx()
    }
}

impl<T> Future for Receiver<T> {
    type Item = T;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<T, Canceled> {
        self.inner.recv()
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.inner.drop_rx()
    }
}
