//! Tokio-02 style MPSC channel.
use std::{
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc::{error::SendError, Permit};

/// Fat sender with `poll_ready` support.
#[derive(Debug)]
// TODO: it doesn't seem we really need 'static here...
pub struct Sender<T: 'static> {
    // field order: `state` can contain reference to `inner`,
    // so it must be dropped first.
    /// State of this sender
    state: State<T>,
    /// Underlying channel
    inner: tokio::sync::mpsc::Sender<T>,
    /// Pinned marker
    pinned: PhantomPinned,
}

impl<T: 'static> Sender<T> {
    /// It is OK to get mutable reference to state, because it is not
    /// self-referencing.
    fn pin_project_state(self: Pin<&mut Self>) -> &mut State<T> {
        unsafe { &mut Pin::into_inner_unchecked(self).state }
    }

    /// Sender must be pinned because state can contain references to it
    unsafe fn pin_project_inner(self: Pin<&mut Self>) -> &mut tokio::sync::mpsc::Sender<T> {
        unsafe { &mut Pin::into_inner_unchecked(self).inner }
    }
}

impl<T: 'static> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            state: State::Empty,
            inner: self.inner.clone(),
            pinned: PhantomPinned,
        }
    }
}

type AcquireFutOutput<T> = Result<Permit<'static, T>, SendError<()>>;

enum State<T: 'static> {
    /// We do not have permit, but we didn't start acquiring it.
    Empty,
    /// We have a permit to send.
    // ALERT: this is self-reference to the Sender.
    Ready(Permit<'static, T>),
    /// We are in process of acquiring a permit
    // ALERT: contained future contains self-reference to the sender.
    Acquire(Pin<Box<dyn Future<Output = AcquireFutOutput<T>>>>),
}

impl<T: 'static> std::fmt::Debug for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Empty => f.debug_tuple("Empty").finish(),
            State::Ready(_) => f.debug_tuple("Ready").field(&"_").finish(),
            State::Acquire(_) => f.debug_tuple("Acquire").field(&"_").finish(),
        }
    }
}

impl<T: 'static> Sender<T> {
    /// Wraps a [tokio sender](tokio::sync::mpsc::Sender).
    pub fn new(inner: tokio::sync::mpsc::Sender<T>) -> Self {
        Sender {
            inner,
            state: State::Empty,
            pinned: PhantomPinned,
        }
    }

    /// Returns sender readiness state
    pub fn is_ready(&self) -> bool {
        matches!(self.state, State::Ready(_))
    }

    /// Sends a message.
    ///
    /// This method panics if the `Sender` is not ready.
    pub fn send(self: Pin<&mut Self>, value: T) {
        let permit = match std::mem::replace(self.pin_project_state(), State::Empty) {
            State::Ready(permit) => permit,
            _ => panic!("called send() on non-ready Sender"),
        };
        permit.send(value);
    }

    /// Disarm permit. This releases the reserved slot in the bounded channel.
    ///
    /// This function can only be called when the `Sender` is ready.
    pub fn disarm(mut self: Pin<&mut Self>) {
        assert!(matches!(self.as_mut().pin_project_state(), State::Ready(_)));
        *self.pin_project_state() = State::Empty;
    }

    /// Tries to acquire a permit.
    ///
    /// This function can not be called when the `Sender` is ready.
    pub fn poll_ready(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), SendError<()>>> {
        let mut fut = match std::mem::replace(self.as_mut().pin_project_state(), State::Empty) {
            State::Ready(_) => panic!("poll_ready() must not be called on ready sender"),
            State::Acquire(f) => f,
            State::Empty => {
                // Extend lifetime here.
                // Future will not outlive sender, neither does permit.
                // `Pin::into_inner_unchecked` is OK because it does not move sender.
                let long_lived_sender = unsafe {
                    std::mem::transmute::<
                        &mut tokio::sync::mpsc::Sender<T>,
                        &'static mut tokio::sync::mpsc::Sender<T>,
                    >(Pin::into_inner_unchecked(
                        self.as_mut().pin_project_inner(),
                    ))
                };
                Box::pin(long_lived_sender.reserve())
            }
        };
        let poll = fut.as_mut().poll(cx);
        match poll {
            Poll::Pending => {
                *self.pin_project_state() = State::Acquire(fut);
                Poll::Pending
            }
            Poll::Ready(Ok(permit)) => {
                *self.pin_project_state() = State::Ready(permit);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(err)) => {
                // leave state in `Empty` state
                Poll::Ready(Err(err))
            }
        }
    }
}

#[cfg(test)]
fn _verify_not_unpin(x: Sender<String>) {
    trait Foo {
        fn is_ready(&self) -> bool;
    }

    impl<T: Unpin> Foo for T {
        fn is_ready(&self) -> bool {
            false
        }
    }

    assert!(x.is_ready());
}
