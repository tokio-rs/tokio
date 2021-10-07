#![cfg_attr(not(feature = "sync"), allow(dead_code, unreachable_pub))]

//! A one-shot channel is used for sending a single message between
//! asynchronous tasks. The [`channel`] function is used to create a
//! [`Sender`] and [`Receiver`] handle pair that form the channel.
//!
//! The `Sender` handle is used by the producer to send the value.
//! The `Receiver` handle is used by the consumer to receive the value.
//!
//! Each handle can be used on separate tasks.
//!
//! # Examples
//!
//! ```
//! use tokio::sync::oneshot;
//!
//! #[tokio::main]
//! async fn main() {
//!     let (tx, rx) = oneshot::channel();
//!
//!     tokio::spawn(async move {
//!         if let Err(_) = tx.send(3) {
//!             println!("the receiver dropped");
//!         }
//!     });
//!
//!     match rx.await {
//!         Ok(v) => println!("got = {:?}", v),
//!         Err(_) => println!("the sender dropped"),
//!     }
//! }
//! ```
//!
//! If the sender is dropped without sending, the receiver will fail with
//! [`error::RecvError`]:
//!
//! ```
//! use tokio::sync::oneshot;
//!
//! #[tokio::main]
//! async fn main() {
//!     let (tx, rx) = oneshot::channel::<u32>();
//!
//!     tokio::spawn(async move {
//!         drop(tx);
//!     });
//!
//!     match rx.await {
//!         Ok(_) => panic!("This doesn't happen"),
//!         Err(_) => println!("the sender dropped"),
//!     }
//! }
//! ```
//!
//! To use a oneshot channel in a `tokio::select!` loop, add `&mut` in front of
//! the channel.
//!
//! ```
//! use tokio::sync::oneshot;
//! use tokio::time::{interval, sleep, Duration};
//!
//! #[tokio::main]
//! # async fn _doc() {}
//! # #[tokio::main(flavor = "current_thread", start_paused = true)]
//! async fn main() {
//!     let (send, mut recv) = oneshot::channel();
//!     let mut interval = interval(Duration::from_millis(100));
//!
//!     # let handle =
//!     tokio::spawn(async move {
//!         sleep(Duration::from_secs(1)).await;
//!         send.send("shut down").unwrap();
//!     });
//!
//!     loop {
//!         tokio::select! {
//!             _ = interval.tick() => println!("Another 100ms"),
//!             msg = &mut recv => {
//!                 println!("Got message: {}", msg.unwrap());
//!                 break;
//!             }
//!         }
//!     }
//!     # handle.await.unwrap();
//! }
//! ```
//!
//! To use a `Sender` from a destructor, put it in an [`Option`] and call
//! [`Option::take`].
//!
//! ```
//! use tokio::sync::oneshot;
//!
//! struct SendOnDrop {
//!     sender: Option<oneshot::Sender<&'static str>>,
//! }
//! impl Drop for SendOnDrop {
//!     fn drop(&mut self) {
//!         if let Some(sender) = self.sender.take() {
//!             // Using `let _ =` to ignore send errors.
//!             let _ = sender.send("I got dropped!");
//!         }
//!     }
//! }
//!
//! #[tokio::main]
//! # async fn _doc() {}
//! # #[tokio::main(flavor = "current_thread")]
//! async fn main() {
//!     let (send, recv) = oneshot::channel();
//!
//!     let send_on_drop = SendOnDrop { sender: Some(send) };
//!     drop(send_on_drop);
//!
//!     assert_eq!(recv.await, Ok("I got dropped!"));
//! }
//! ```

use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Arc;

use std::fmt;
use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::sync::atomic::Ordering::{self, AcqRel, Acquire};
use std::task::Poll::{Pending, Ready};
use std::task::{Context, Poll, Waker};

/// Sends a value to the associated [`Receiver`].
///
/// A pair of both a [`Sender`] and a [`Receiver`]  are created by the
/// [`channel`](fn@channel) function.
///
/// # Examples
///
/// ```
/// use tokio::sync::oneshot;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, rx) = oneshot::channel();
///
///     tokio::spawn(async move {
///         if let Err(_) = tx.send(3) {
///             println!("the receiver dropped");
///         }
///     });
///
///     match rx.await {
///         Ok(v) => println!("got = {:?}", v),
///         Err(_) => println!("the sender dropped"),
///     }
/// }
/// ```
///
/// If the sender is dropped without sending, the receiver will fail with
/// [`error::RecvError`]:
///
/// ```
/// use tokio::sync::oneshot;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, rx) = oneshot::channel::<u32>();
///
///     tokio::spawn(async move {
///         drop(tx);
///     });
///
///     match rx.await {
///         Ok(_) => panic!("This doesn't happen"),
///         Err(_) => println!("the sender dropped"),
///     }
/// }
/// ```
///
/// To use a `Sender` from a destructor, put it in an [`Option`] and call
/// [`Option::take`].
///
/// ```
/// use tokio::sync::oneshot;
///
/// struct SendOnDrop {
///     sender: Option<oneshot::Sender<&'static str>>,
/// }
/// impl Drop for SendOnDrop {
///     fn drop(&mut self) {
///         if let Some(sender) = self.sender.take() {
///             // Using `let _ =` to ignore send errors.
///             let _ = sender.send("I got dropped!");
///         }
///     }
/// }
///
/// #[tokio::main]
/// # async fn _doc() {}
/// # #[tokio::main(flavor = "current_thread")]
/// async fn main() {
///     let (send, recv) = oneshot::channel();
///
///     let send_on_drop = SendOnDrop { sender: Some(send) };
///     drop(send_on_drop);
///
///     assert_eq!(recv.await, Ok("I got dropped!"));
/// }
/// ```
///
/// [`Option`]: std::option::Option
/// [`Option::take`]: std::option::Option::take
#[derive(Debug)]
pub struct Sender<T> {
    inner: Option<Arc<Inner<T>>>,
}

/// Receive a value from the associated [`Sender`].
///
/// A pair of both a [`Sender`] and a [`Receiver`]  are created by the
/// [`channel`](fn@channel) function.
///
/// This channel has no `recv` method because the receiver itself implements the
/// [`Future`] trait. To receive a value, `.await` the `Receiver` object directly.
///
/// [`Future`]: trait@std::future::Future
///
/// # Examples
///
/// ```
/// use tokio::sync::oneshot;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, rx) = oneshot::channel();
///
///     tokio::spawn(async move {
///         if let Err(_) = tx.send(3) {
///             println!("the receiver dropped");
///         }
///     });
///
///     match rx.await {
///         Ok(v) => println!("got = {:?}", v),
///         Err(_) => println!("the sender dropped"),
///     }
/// }
/// ```
///
/// If the sender is dropped without sending, the receiver will fail with
/// [`error::RecvError`]:
///
/// ```
/// use tokio::sync::oneshot;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, rx) = oneshot::channel::<u32>();
///
///     tokio::spawn(async move {
///         drop(tx);
///     });
///
///     match rx.await {
///         Ok(_) => panic!("This doesn't happen"),
///         Err(_) => println!("the sender dropped"),
///     }
/// }
/// ```
///
/// To use a `Receiver` in a `tokio::select!` loop, add `&mut` in front of the
/// channel.
///
/// ```
/// use tokio::sync::oneshot;
/// use tokio::time::{interval, sleep, Duration};
///
/// #[tokio::main]
/// # async fn _doc() {}
/// # #[tokio::main(flavor = "current_thread", start_paused = true)]
/// async fn main() {
///     let (send, mut recv) = oneshot::channel();
///     let mut interval = interval(Duration::from_millis(100));
///
///     # let handle =
///     tokio::spawn(async move {
///         sleep(Duration::from_secs(1)).await;
///         send.send("shut down").unwrap();
///     });
///
///     loop {
///         tokio::select! {
///             _ = interval.tick() => println!("Another 100ms"),
///             msg = &mut recv => {
///                 println!("Got message: {}", msg.unwrap());
///                 break;
///             }
///         }
///     }
///     # handle.await.unwrap();
/// }
/// ```
#[derive(Debug)]
pub struct Receiver<T> {
    inner: Option<Arc<Inner<T>>>,
}

pub mod error {
    //! Oneshot error types

    use std::fmt;

    /// Error returned by the `Future` implementation for `Receiver`.
    #[derive(Debug, Eq, PartialEq)]
    pub struct RecvError(pub(super) ());

    /// Error returned by the `try_recv` function on `Receiver`.
    #[derive(Debug, Eq, PartialEq)]
    pub enum TryRecvError {
        /// The send half of the channel has not yet sent a value.
        Empty,

        /// The send half of the channel was dropped without sending a value.
        Closed,
    }

    // ===== impl RecvError =====

    impl fmt::Display for RecvError {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "channel closed")
        }
    }

    impl std::error::Error for RecvError {}

    // ===== impl TryRecvError =====

    impl fmt::Display for TryRecvError {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                TryRecvError::Empty => write!(fmt, "channel empty"),
                TryRecvError::Closed => write!(fmt, "channel closed"),
            }
        }
    }

    impl std::error::Error for TryRecvError {}
}

use self::error::*;

struct Inner<T> {
    /// Manages the state of the inner cell
    state: AtomicUsize,

    /// The value. This is set by `Sender` and read by `Receiver`. The state of
    /// the cell is tracked by `state`.
    value: UnsafeCell<Option<T>>,

    /// The task to notify when the receiver drops without consuming the value.
    tx_task: Task,

    /// The task to notify when the value is sent.
    rx_task: Task,
}

struct Task(UnsafeCell<MaybeUninit<Waker>>);

impl Task {
    unsafe fn will_wake(&self, cx: &mut Context<'_>) -> bool {
        self.with_task(|w| w.will_wake(cx.waker()))
    }

    unsafe fn with_task<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Waker) -> R,
    {
        self.0.with(|ptr| {
            let waker: *const Waker = (&*ptr).as_ptr();
            f(&*waker)
        })
    }

    unsafe fn drop_task(&self) {
        self.0.with_mut(|ptr| {
            let ptr: *mut Waker = (&mut *ptr).as_mut_ptr();
            ptr.drop_in_place();
        });
    }

    unsafe fn set_task(&self, cx: &mut Context<'_>) {
        self.0.with_mut(|ptr| {
            let ptr: *mut Waker = (&mut *ptr).as_mut_ptr();
            ptr.write(cx.waker().clone());
        });
    }
}

#[derive(Clone, Copy)]
struct State(usize);

/// Create a new one-shot channel for sending single values across asynchronous
/// tasks.
///
/// The function returns separate "send" and "receive" handles. The `Sender`
/// handle is used by the producer to send the value. The `Receiver` handle is
/// used by the consumer to receive the value.
///
/// Each handle can be used on separate tasks.
///
/// # Examples
///
/// ```
/// use tokio::sync::oneshot;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, rx) = oneshot::channel();
///
///     tokio::spawn(async move {
///         if let Err(_) = tx.send(3) {
///             println!("the receiver dropped");
///         }
///     });
///
///     match rx.await {
///         Ok(v) => println!("got = {:?}", v),
///         Err(_) => println!("the sender dropped"),
///     }
/// }
/// ```
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        state: AtomicUsize::new(State::new().as_usize()),
        value: UnsafeCell::new(None),
        tx_task: Task(UnsafeCell::new(MaybeUninit::uninit())),
        rx_task: Task(UnsafeCell::new(MaybeUninit::uninit())),
    });

    let tx = Sender {
        inner: Some(inner.clone()),
    };
    let rx = Receiver { inner: Some(inner) };

    (tx, rx)
}

impl<T> Sender<T> {
    /// Attempts to send a value on this channel, returning it back if it could
    /// not be sent.
    ///
    /// This method consumes `self` as only one value may ever be sent on a oneshot
    /// channel. It is not marked async because sending a message to an oneshot
    /// channel never requires any form of waiting.  Because of this, the `send`
    /// method can be used in both synchronous and asynchronous code without
    /// problems.
    ///
    /// A successful send occurs when it is determined that the other end of the
    /// channel has not hung up already. An unsuccessful send would be one where
    /// the corresponding receiver has already been deallocated. Note that a
    /// return value of `Err` means that the data will never be received, but
    /// a return value of `Ok` does *not* mean that the data will be received.
    /// It is possible for the corresponding receiver to hang up immediately
    /// after this function returns `Ok`.
    ///
    /// # Examples
    ///
    /// Send a value to another task
    ///
    /// ```
    /// use tokio::sync::oneshot;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, rx) = oneshot::channel();
    ///
    ///     tokio::spawn(async move {
    ///         if let Err(_) = tx.send(3) {
    ///             println!("the receiver dropped");
    ///         }
    ///     });
    ///
    ///     match rx.await {
    ///         Ok(v) => println!("got = {:?}", v),
    ///         Err(_) => println!("the sender dropped"),
    ///     }
    /// }
    /// ```
    pub fn send(mut self, t: T) -> Result<(), T> {
        let inner = self.inner.take().unwrap();

        inner.value.with_mut(|ptr| unsafe {
            *ptr = Some(t);
        });

        if !inner.complete() {
            unsafe {
                return Err(inner.consume_value().unwrap());
            }
        }

        Ok(())
    }

    /// Waits for the associated [`Receiver`] handle to close.
    ///
    /// A [`Receiver`] is closed by either calling [`close`] explicitly or the
    /// [`Receiver`] value is dropped.
    ///
    /// This function is useful when paired with `select!` to abort a
    /// computation when the receiver is no longer interested in the result.
    ///
    /// # Return
    ///
    /// Returns a `Future` which must be awaited on.
    ///
    /// [`Receiver`]: Receiver
    /// [`close`]: Receiver::close
    ///
    /// # Examples
    ///
    /// Basic usage
    ///
    /// ```
    /// use tokio::sync::oneshot;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (mut tx, rx) = oneshot::channel::<()>();
    ///
    ///     tokio::spawn(async move {
    ///         drop(rx);
    ///     });
    ///
    ///     tx.closed().await;
    ///     println!("the receiver dropped");
    /// }
    /// ```
    ///
    /// Paired with select
    ///
    /// ```
    /// use tokio::sync::oneshot;
    /// use tokio::time::{self, Duration};
    ///
    /// async fn compute() -> String {
    ///     // Complex computation returning a `String`
    /// # "hello".to_string()
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (mut tx, rx) = oneshot::channel();
    ///
    ///     tokio::spawn(async move {
    ///         tokio::select! {
    ///             _ = tx.closed() => {
    ///                 // The receiver dropped, no need to do any further work
    ///             }
    ///             value = compute() => {
    ///                 // The send can fail if the channel was closed at the exact same
    ///                 // time as when compute() finished, so just ignore the failure.
    ///                 let _ = tx.send(value);
    ///             }
    ///         }
    ///     });
    ///
    ///     // Wait for up to 10 seconds
    ///     let _ = time::timeout(Duration::from_secs(10), rx).await;
    /// }
    /// ```
    pub async fn closed(&mut self) {
        use crate::future::poll_fn;

        poll_fn(|cx| self.poll_closed(cx)).await
    }

    /// Returns `true` if the associated [`Receiver`] handle has been dropped.
    ///
    /// A [`Receiver`] is closed by either calling [`close`] explicitly or the
    /// [`Receiver`] value is dropped.
    ///
    /// If `true` is returned, a call to `send` will always result in an error.
    ///
    /// [`Receiver`]: Receiver
    /// [`close`]: Receiver::close
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::oneshot;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, rx) = oneshot::channel();
    ///
    ///     assert!(!tx.is_closed());
    ///
    ///     drop(rx);
    ///
    ///     assert!(tx.is_closed());
    ///     assert!(tx.send("never received").is_err());
    /// }
    /// ```
    pub fn is_closed(&self) -> bool {
        let inner = self.inner.as_ref().unwrap();

        let state = State::load(&inner.state, Acquire);
        state.is_closed()
    }

    /// Check whether the oneshot channel has been closed, and if not, schedules the
    /// `Waker` in the provided `Context` to receive a notification when the channel is
    /// closed.
    ///
    /// A [`Receiver`] is closed by either calling [`close`] explicitly, or when the
    /// [`Receiver`] value is dropped.
    ///
    /// Note that on multiple calls to poll, only the `Waker` from the `Context` passed
    /// to the most recent call will be scheduled to receive a wakeup.
    ///
    /// [`Receiver`]: struct@crate::sync::oneshot::Receiver
    /// [`close`]: fn@crate::sync::oneshot::Receiver::close
    ///
    /// # Return value
    ///
    /// This function returns:
    ///
    ///  * `Poll::Pending` if the channel is still open.
    ///  * `Poll::Ready(())` if the channel is closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::oneshot;
    ///
    /// use futures::future::poll_fn;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (mut tx, mut rx) = oneshot::channel::<()>();
    ///
    ///     tokio::spawn(async move {
    ///         rx.close();
    ///     });
    ///
    ///     poll_fn(|cx| tx.poll_closed(cx)).await;
    ///
    ///     println!("the receiver dropped");
    /// }
    /// ```
    pub fn poll_closed(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        // Keep track of task budget
        let coop = ready!(crate::coop::poll_proceed(cx));

        let inner = self.inner.as_ref().unwrap();

        let mut state = State::load(&inner.state, Acquire);

        if state.is_closed() {
            coop.made_progress();
            return Poll::Ready(());
        }

        if state.is_tx_task_set() {
            let will_notify = unsafe { inner.tx_task.will_wake(cx) };

            if !will_notify {
                state = State::unset_tx_task(&inner.state);

                if state.is_closed() {
                    // Set the flag again so that the waker is released in drop
                    State::set_tx_task(&inner.state);
                    coop.made_progress();
                    return Ready(());
                } else {
                    unsafe { inner.tx_task.drop_task() };
                }
            }
        }

        if !state.is_tx_task_set() {
            // Attempt to set the task
            unsafe {
                inner.tx_task.set_task(cx);
            }

            // Update the state
            state = State::set_tx_task(&inner.state);

            if state.is_closed() {
                coop.made_progress();
                return Ready(());
            }
        }

        Pending
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.as_ref() {
            inner.complete();
        }
    }
}

impl<T> Receiver<T> {
    /// Prevents the associated [`Sender`] handle from sending a value.
    ///
    /// Any `send` operation which happens after calling `close` is guaranteed
    /// to fail. After calling `close`, [`try_recv`] should be called to
    /// receive a value if one was sent **before** the call to `close`
    /// completed.
    ///
    /// This function is useful to perform a graceful shutdown and ensure that a
    /// value will not be sent into the channel and never received.
    ///
    /// `close` is no-op if a message is already received or the channel
    /// is already closed.
    ///
    /// [`Sender`]: Sender
    /// [`try_recv`]: Receiver::try_recv
    ///
    /// # Examples
    ///
    /// Prevent a value from being sent
    ///
    /// ```
    /// use tokio::sync::oneshot;
    /// use tokio::sync::oneshot::error::TryRecvError;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = oneshot::channel();
    ///
    ///     assert!(!tx.is_closed());
    ///
    ///     rx.close();
    ///
    ///     assert!(tx.is_closed());
    ///     assert!(tx.send("never received").is_err());
    ///
    ///     match rx.try_recv() {
    ///         Err(TryRecvError::Closed) => {}
    ///         _ => unreachable!(),
    ///     }
    /// }
    /// ```
    ///
    /// Receive a value sent **before** calling `close`
    ///
    /// ```
    /// use tokio::sync::oneshot;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = oneshot::channel();
    ///
    ///     assert!(tx.send("will receive").is_ok());
    ///
    ///     rx.close();
    ///
    ///     let msg = rx.try_recv().unwrap();
    ///     assert_eq!(msg, "will receive");
    /// }
    /// ```
    pub fn close(&mut self) {
        if let Some(inner) = self.inner.as_ref() {
            inner.close();
        }
    }

    /// Attempts to receive a value.
    ///
    /// If a pending value exists in the channel, it is returned. If no value
    /// has been sent, the current task **will not** be registered for
    /// future notification.
    ///
    /// This function is useful to call from outside the context of an
    /// asynchronous task.
    ///
    /// # Return
    ///
    /// - `Ok(T)` if a value is pending in the channel.
    /// - `Err(TryRecvError::Empty)` if no value has been sent yet.
    /// - `Err(TryRecvError::Closed)` if the sender has dropped without sending
    ///   a value.
    ///
    /// # Examples
    ///
    /// `try_recv` before a value is sent, then after.
    ///
    /// ```
    /// use tokio::sync::oneshot;
    /// use tokio::sync::oneshot::error::TryRecvError;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = oneshot::channel();
    ///
    ///     match rx.try_recv() {
    ///         // The channel is currently empty
    ///         Err(TryRecvError::Empty) => {}
    ///         _ => unreachable!(),
    ///     }
    ///
    ///     // Send a value
    ///     tx.send("hello").unwrap();
    ///
    ///     match rx.try_recv() {
    ///         Ok(value) => assert_eq!(value, "hello"),
    ///         _ => unreachable!(),
    ///     }
    /// }
    /// ```
    ///
    /// `try_recv` when the sender dropped before sending a value
    ///
    /// ```
    /// use tokio::sync::oneshot;
    /// use tokio::sync::oneshot::error::TryRecvError;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = oneshot::channel::<()>();
    ///
    ///     drop(tx);
    ///
    ///     match rx.try_recv() {
    ///         // The channel will never receive a value.
    ///         Err(TryRecvError::Closed) => {}
    ///         _ => unreachable!(),
    ///     }
    /// }
    /// ```
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let result = if let Some(inner) = self.inner.as_ref() {
            let state = State::load(&inner.state, Acquire);

            if state.is_complete() {
                match unsafe { inner.consume_value() } {
                    Some(value) => Ok(value),
                    None => Err(TryRecvError::Closed),
                }
            } else if state.is_closed() {
                Err(TryRecvError::Closed)
            } else {
                // Not ready, this does not clear `inner`
                return Err(TryRecvError::Empty);
            }
        } else {
            Err(TryRecvError::Closed)
        };

        self.inner = None;
        result
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.as_ref() {
            inner.close();
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // If `inner` is `None`, then `poll()` has already completed.
        let ret = if let Some(inner) = self.as_ref().get_ref().inner.as_ref() {
            ready!(inner.poll_recv(cx))?
        } else {
            panic!("called after complete");
        };

        self.inner = None;
        Ready(Ok(ret))
    }
}

impl<T> Inner<T> {
    fn complete(&self) -> bool {
        let prev = State::set_complete(&self.state);

        if prev.is_closed() {
            return false;
        }

        if prev.is_rx_task_set() {
            // TODO: Consume waker?
            unsafe {
                self.rx_task.with_task(Waker::wake_by_ref);
            }
        }

        true
    }

    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        // Keep track of task budget
        let coop = ready!(crate::coop::poll_proceed(cx));

        // Load the state
        let mut state = State::load(&self.state, Acquire);

        if state.is_complete() {
            coop.made_progress();
            match unsafe { self.consume_value() } {
                Some(value) => Ready(Ok(value)),
                None => Ready(Err(RecvError(()))),
            }
        } else if state.is_closed() {
            coop.made_progress();
            Ready(Err(RecvError(())))
        } else {
            if state.is_rx_task_set() {
                let will_notify = unsafe { self.rx_task.will_wake(cx) };

                // Check if the task is still the same
                if !will_notify {
                    // Unset the task
                    state = State::unset_rx_task(&self.state);
                    if state.is_complete() {
                        // Set the flag again so that the waker is released in drop
                        State::set_rx_task(&self.state);

                        coop.made_progress();
                        return match unsafe { self.consume_value() } {
                            Some(value) => Ready(Ok(value)),
                            None => Ready(Err(RecvError(()))),
                        };
                    } else {
                        unsafe { self.rx_task.drop_task() };
                    }
                }
            }

            if !state.is_rx_task_set() {
                // Attempt to set the task
                unsafe {
                    self.rx_task.set_task(cx);
                }

                // Update the state
                state = State::set_rx_task(&self.state);

                if state.is_complete() {
                    coop.made_progress();
                    match unsafe { self.consume_value() } {
                        Some(value) => Ready(Ok(value)),
                        None => Ready(Err(RecvError(()))),
                    }
                } else {
                    Pending
                }
            } else {
                Pending
            }
        }
    }

    /// Called by `Receiver` to indicate that the value will never be received.
    fn close(&self) {
        let prev = State::set_closed(&self.state);

        if prev.is_tx_task_set() && !prev.is_complete() {
            unsafe {
                self.tx_task.with_task(Waker::wake_by_ref);
            }
        }
    }

    /// Consumes the value. This function does not check `state`.
    unsafe fn consume_value(&self) -> Option<T> {
        self.value.with_mut(|ptr| (*ptr).take())
    }
}

unsafe impl<T: Send> Send for Inner<T> {}
unsafe impl<T: Send> Sync for Inner<T> {}

fn mut_load(this: &mut AtomicUsize) -> usize {
    this.with_mut(|v| *v)
}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        let state = State(mut_load(&mut self.state));

        if state.is_rx_task_set() {
            unsafe {
                self.rx_task.drop_task();
            }
        }

        if state.is_tx_task_set() {
            unsafe {
                self.tx_task.drop_task();
            }
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Inner<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::sync::atomic::Ordering::Relaxed;

        fmt.debug_struct("Inner")
            .field("state", &State::load(&self.state, Relaxed))
            .finish()
    }
}

const RX_TASK_SET: usize = 0b00001;
const VALUE_SENT: usize = 0b00010;
const CLOSED: usize = 0b00100;
const TX_TASK_SET: usize = 0b01000;

impl State {
    fn new() -> State {
        State(0)
    }

    fn is_complete(self) -> bool {
        self.0 & VALUE_SENT == VALUE_SENT
    }

    fn set_complete(cell: &AtomicUsize) -> State {
        // TODO: This could be `Release`, followed by an `Acquire` fence *if*
        // the `RX_TASK_SET` flag is set. However, `loom` does not support
        // fences yet.
        let val = cell.fetch_or(VALUE_SENT, AcqRel);
        State(val)
    }

    fn is_rx_task_set(self) -> bool {
        self.0 & RX_TASK_SET == RX_TASK_SET
    }

    fn set_rx_task(cell: &AtomicUsize) -> State {
        let val = cell.fetch_or(RX_TASK_SET, AcqRel);
        State(val | RX_TASK_SET)
    }

    fn unset_rx_task(cell: &AtomicUsize) -> State {
        let val = cell.fetch_and(!RX_TASK_SET, AcqRel);
        State(val & !RX_TASK_SET)
    }

    fn is_closed(self) -> bool {
        self.0 & CLOSED == CLOSED
    }

    fn set_closed(cell: &AtomicUsize) -> State {
        // Acquire because we want all later writes (attempting to poll) to be
        // ordered after this.
        let val = cell.fetch_or(CLOSED, Acquire);
        State(val)
    }

    fn set_tx_task(cell: &AtomicUsize) -> State {
        let val = cell.fetch_or(TX_TASK_SET, AcqRel);
        State(val | TX_TASK_SET)
    }

    fn unset_tx_task(cell: &AtomicUsize) -> State {
        let val = cell.fetch_and(!TX_TASK_SET, AcqRel);
        State(val & !TX_TASK_SET)
    }

    fn is_tx_task_set(self) -> bool {
        self.0 & TX_TASK_SET == TX_TASK_SET
    }

    fn as_usize(self) -> usize {
        self.0
    }

    fn load(cell: &AtomicUsize, order: Ordering) -> State {
        let val = cell.load(order);
        State(val)
    }
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("State")
            .field("is_complete", &self.is_complete())
            .field("is_closed", &self.is_closed())
            .field("is_rx_task_set", &self.is_rx_task_set())
            .field("is_tx_task_set", &self.is_tx_task_set())
            .finish()
    }
}
