//! Low-level utilities for `poll`-ifying `async fn`-based apis.

use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
    task::{Context, Poll},
};

/// Marker type used as a privacy proof for `AsyncOp::op`.
/// You should never construct this type.
#[derive(Debug)]
pub struct Internal(());

/// Type that has `async fn`.
/// `Pollify` can wrap instance of such type and
/// provide `poll`-based api.
pub trait AsyncOp {
    /// Future returned by this async fn.
    /// Can be selected as `Box<dyn std::future::Future<Output=/*return type*/>>`
    type Fut: Future;

    /// Creates a future.
    /// # Safety
    /// This function is unsafe to call
    /// Calling `op` exclusively borrows for the whole lifetime
    /// of returned future. This will be expressed using generic associated
    /// types which are not stable yet.
    unsafe fn start_operation(&mut self) -> Self::Fut;
}

enum State<F: Future> {
    /// We have neither future nor its result.
    Empty,
    /// We have a ready output.
    // ALERT: this is self-reference to the `inner`.
    Ready(F::Output),
    /// We have a future in progress
    // ALERT: contained future references both itself and `inner`.
    Progress(F),
}

enum StateKind {
    Empty,
    Ready,
}

impl<F: Future> State<F> {
    fn is_in_progress(&self) -> bool {
        matches!(self, State::Progress(_))
    }

    fn pinned_take(self: Pin<&mut Self>) -> Result<Self, Pin<&mut F>> {
        if self.is_in_progress() {
            unsafe {
                Err(self.map_unchecked_mut(|this| match this {
                    State::Progress(f) => f,
                    _ => unreachable!(),
                }))
            }
        } else {
            let this = unsafe { Pin::into_inner_unchecked(self) };
            Ok(std::mem::replace(this, Self::Empty))
        }
    }

    fn reset(mut self: Pin<&mut Self>) -> StateKind {
        if self.is_in_progress() {
            self.set(State::Empty);
            StateKind::Empty
        } else {
            let this = unsafe { Pin::into_inner_unchecked(self.as_mut()) };
            let kind = match this {
                State::Empty => StateKind::Empty,
                State::Ready(_) => StateKind::Ready,
                State::Progress(_) => unreachable!(),
            };
            self.set(State::Empty);
            kind
        }
    }
}

impl<F: Future> Debug for State<F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            State::Empty => f.debug_tuple("Empty").finish(),
            State::Ready(_) => f.debug_tuple("Ready").field(&"_").finish(),
            State::Progress(_) => f.debug_tuple("Progress").field(&"_").finish(),
        }
    }
}

/// Wraps a type with `async fn` and exposes `poll` for it.
pub struct Pollify<A: AsyncOp> {
    inner: A,
    state: State<A::Fut>,
    _pinned: PhantomPinned,
}

impl<A: AsyncOp + Debug> Debug for Pollify<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Pollify")
            .field("inner", &self.inner)
            .field("state", &self.state)
            .finish()
    }
}

impl<A: AsyncOp> Pollify<A> {
    /// State must be pinned when it is in `Progress` state.
    fn pin_project_state(self: Pin<&mut Self>) -> Pin<&mut State<A::Fut>> {
        unsafe { self.map_unchecked_mut(|this| &mut this.state) }
    }

    /// Inner must be pinned because state can contain references to it
    fn pin_project_inner(self: Pin<&mut Self>) -> Pin<&mut A> {
        unsafe { self.map_unchecked_mut(|this| &mut this.inner) }
    }
}

impl<A: AsyncOp> Pollify<A> {
    /// Wraps a [tokio sender](tokio::sync::mpsc::Sender).
    pub fn new(inner: A) -> Self {
        Pollify {
            inner,
            state: State::Empty,
            _pinned: PhantomPinned,
        }
    }

    /// Returns sender readiness state
    pub fn is_ready(&self) -> bool {
        matches!(self.state, State::Ready(_))
    }

    /// Extracts operation result. Pollify is now in empty state
    /// and can be polled again.
    /// # Panics
    /// This method panics if the `Sender` is not ready.
    pub fn extract(self: Pin<&mut Self>) -> <A::Fut as Future>::Output {
        match self.pin_project_state().pinned_take() {
            Ok(State::Ready(out)) => out,
            _ => panic!("extract() called, but no output available"),
        }
    }

    /// Returns reference to future result.
    pub fn output_ref(&self) -> &<A::Fut as Future>::Output {
        match &self.state {
            State::Ready(output) => output,
            _ => panic!("output_ref() called, but no output available"),
        }
    }

    /// Cancels operation. Pollify is now in empty state
    /// and can be polled again.
    ///
    /// Returns true if before the call operation was completed.
    pub fn cancel(mut self: Pin<&mut Self>) -> bool {
        let was_ready = matches!(self.as_mut().pin_project_state().reset(), StateKind::Ready);
        was_ready
    }

    /// Polls current async operation, starting one if necessary.
    ///
    /// This function can not be called when the `Pollify` is ready.
    pub fn poll<'a>(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()>
    where
        A: 'a,
    {
        // let state = unsafe { Pin::into_inner_unchecked(self.as_mut().pin_project_state()) };

        let fut = match self.as_mut().pin_project_state().pinned_take() {
            Ok(State::Ready(_)) => panic!("poll_ready() must not be called on ready sender"),
            Ok(State::Progress(_)) => unreachable!(),
            Ok(State::Empty) => {
                let inner = self.as_mut().pin_project_inner();
                let fut = unsafe { Pin::into_inner_unchecked(inner).start_operation() };
                self.as_mut().pin_project_state().set(State::Progress(fut));
                self.as_mut().pin_project_state().pinned_take().unwrap_err()
            }
            Err(fut) => fut,
        };
        let poll = fut.poll(cx);
        match poll {
            Poll::Pending => Poll::Pending,
            Poll::Ready(output) => {
                self.pin_project_state().set(State::Ready(output));
                Poll::Ready(())
            }
        }
    }

    /// Returns reference to wrapped value
    pub fn inner(&self) -> &A {
        &self.inner
    }
}
