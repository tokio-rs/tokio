#![cfg_attr(not(feature = "sync"), allow(dead_code, unreachable_pub))]

//! A channel for sending a single message between asynchronous tasks.

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

/// Sends a value to the associated `Receiver`.
///
/// Instances are created by the [`channel`](fn@channel) function.
#[derive(Debug)]
pub struct Sender<T> {
    inner: Option<Arc<Inner<T>>>,
}

/// Receive a value from the associated `Sender`.
///
/// Instances are created by the [`channel`](fn@channel) function.
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
    tx_task: UnsafeCell<MaybeUninit<Waker>>,

    /// The task to notify when the value is sent.
    rx_task: UnsafeCell<MaybeUninit<Waker>>,
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
    #[allow(deprecated)]
    let inner = Arc::new(Inner {
        state: AtomicUsize::new(State::new().as_usize()),
        value: UnsafeCell::new(None),
        tx_task: UnsafeCell::new(MaybeUninit::uninit()),
        rx_task: UnsafeCell::new(MaybeUninit::uninit()),
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
    /// The function consumes `self` as only one value may ever be sent on a
    /// one-shot channel.
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
            return Err(inner
                .value
                .with_mut(|ptr| unsafe { (*ptr).take() }.unwrap()));
        }

        Ok(())
    }

    #[doc(hidden)] // TODO: remove
    pub fn poll_closed(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        // Keep track of task budget
        ready!(crate::coop::poll_proceed(cx));

        let inner = self.inner.as_ref().unwrap();

        let mut state = State::load(&inner.state, Acquire);

        if state.is_closed() {
            return Poll::Ready(());
        }

        if state.is_tx_task_set() {
            let will_notify = unsafe { inner.with_tx_task(|w| w.will_wake(cx.waker())) };

            if !will_notify {
                state = State::unset_tx_task(&inner.state);

                if state.is_closed() {
                    // Set the flag again so that the waker is released in drop
                    State::set_tx_task(&inner.state);
                    return Ready(());
                } else {
                    unsafe { inner.drop_tx_task() };
                }
            }
        }

        if !state.is_tx_task_set() {
            // Attempt to set the task
            unsafe {
                inner.set_tx_task(cx);
            }

            // Update the state
            state = State::set_tx_task(&inner.state);

            if state.is_closed() {
                return Ready(());
            }
        }

        Pending
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
    /// use futures::{select, FutureExt};
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
    ///         select! {
    ///             _ = tx.closed().fuse() => {
    ///                 // The receiver dropped, no need to do any further work
    ///             }
    ///             value = compute().fuse() => {
    ///                 tx.send(value).unwrap()
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
    /// to fail. After calling `close`, `Receiver::poll`] should be called to
    /// receive a value if one was sent **before** the call to `close`
    /// completed.
    ///
    /// This function is useful to perform a graceful shutdown and ensure that a
    /// value will not be sent into the channel and never received.
    ///
    /// [`Sender`]: Sender
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
        let inner = self.inner.as_ref().unwrap();
        inner.close();
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
            panic!("called after complete");
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
                self.with_rx_task(Waker::wake_by_ref);
            }
        }

        true
    }

    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        // Keep track of task budget
        ready!(crate::coop::poll_proceed(cx));

        // Load the state
        let mut state = State::load(&self.state, Acquire);

        if state.is_complete() {
            match unsafe { self.consume_value() } {
                Some(value) => Ready(Ok(value)),
                None => Ready(Err(RecvError(()))),
            }
        } else if state.is_closed() {
            Ready(Err(RecvError(())))
        } else {
            if state.is_rx_task_set() {
                let will_notify = unsafe { self.with_rx_task(|w| w.will_wake(cx.waker())) };

                // Check if the task is still the same
                if !will_notify {
                    // Unset the task
                    state = State::unset_rx_task(&self.state);
                    if state.is_complete() {
                        // Set the flag again so that the waker is released in drop
                        State::set_rx_task(&self.state);

                        return match unsafe { self.consume_value() } {
                            Some(value) => Ready(Ok(value)),
                            None => Ready(Err(RecvError(()))),
                        };
                    } else {
                        unsafe { self.drop_rx_task() };
                    }
                }
            }

            if !state.is_rx_task_set() {
                // Attempt to set the task
                unsafe {
                    self.set_rx_task(cx);
                }

                // Update the state
                state = State::set_rx_task(&self.state);

                if state.is_complete() {
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
                self.with_tx_task(Waker::wake_by_ref);
            }
        }
    }

    /// Consumes the value. This function does not check `state`.
    unsafe fn consume_value(&self) -> Option<T> {
        self.value.with_mut(|ptr| (*ptr).take())
    }

    unsafe fn with_rx_task<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Waker) -> R,
    {
        self.rx_task.with(|ptr| {
            let waker: *const Waker = (&*ptr).as_ptr();
            f(&*waker)
        })
    }

    unsafe fn with_tx_task<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Waker) -> R,
    {
        self.tx_task.with(|ptr| {
            let waker: *const Waker = (&*ptr).as_ptr();
            f(&*waker)
        })
    }

    unsafe fn drop_rx_task(&self) {
        self.rx_task.with_mut(|ptr| {
            let ptr: *mut Waker = (&mut *ptr).as_mut_ptr();
            ptr.drop_in_place();
        });
    }

    unsafe fn drop_tx_task(&self) {
        self.tx_task.with_mut(|ptr| {
            let ptr: *mut Waker = (&mut *ptr).as_mut_ptr();
            ptr.drop_in_place();
        });
    }

    unsafe fn set_rx_task(&self, cx: &mut Context<'_>) {
        self.rx_task.with_mut(|ptr| {
            let ptr: *mut Waker = (&mut *ptr).as_mut_ptr();
            ptr.write(cx.waker().clone());
        });
    }

    unsafe fn set_tx_task(&self, cx: &mut Context<'_>) {
        self.tx_task.with_mut(|ptr| {
            let ptr: *mut Waker = (&mut *ptr).as_mut_ptr();
            ptr.write(cx.waker().clone());
        });
    }
}

unsafe impl<T: Send> Send for Inner<T> {}
unsafe impl<T: Send> Sync for Inner<T> {}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        let state = State(self.state.with_mut(|v| *v));

        if state.is_rx_task_set() {
            unsafe {
                self.drop_rx_task();
            }
        }

        if state.is_tx_task_set() {
            unsafe {
                self.drop_tx_task();
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
