//! Tokio-02 style MPSC channel.
use crate::pollify::Pollify;
use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc::{error::SendError, Permit};

/// Wrapper for Box<dyn Future> conditionally implementing Send and Sync
struct FutureBox<'a, T, M>(Pin<Box<dyn Future<Output = T> + 'a>>, PhantomData<Box<M>>);

impl<'a, T, M> Future for FutureBox<'a, T, M> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut Pin::into_inner(self).0).as_mut().poll(cx)
    }
}

unsafe impl<'a, T, M> Send for FutureBox<'a, T, M> where M: Send {}
unsafe impl<'a, T, M> Sync for FutureBox<'a, T, M> where M: Sync {}

#[derive(Debug)]
struct Chan<'a, T>(tokio::sync::mpsc::Sender<T>, PhantomData<&'a ()>);
impl<'a, T: 'a> crate::pollify::AsyncOp for Chan<'a, T> {
    type Fut = FutureBox<'a, AcquireFutOutput<'a, T>, T>;

    unsafe fn start_operation(&mut self) -> Self::Fut {
        // SAFETY: guaranteed by AsyncOp invariant.
        let long_lived_sender = std::mem::transmute::<
            &mut tokio::sync::mpsc::Sender<T>,
            &'a mut tokio::sync::mpsc::Sender<T>,
        >(&mut self.0);
        FutureBox(Box::pin(long_lived_sender.reserve()), PhantomData)
    }
}

/// Fat sender with `poll_ready` support.
#[derive(Debug)]
pub struct Sender<'a, T: Send + 'a> {
    used_in_poll_ready: PhantomData<&'a ()>,
    pollify: Pollify<Chan<'a, T>>,
}

impl<'a, T: Send + 'a> Clone for Sender<'a, T> {
    fn clone(&self) -> Self {
        Self::new(self.pollify.inner().0.clone())
    }
}

type AcquireFutOutput<'a, T> = Result<Permit<'a, T>, SendError<()>>;

impl<'a, T: Send + 'a> Sender<'a, T> {
    /// Wraps a [tokio sender](tokio::sync::mpsc::Sender).
    pub fn new(inner: tokio::sync::mpsc::Sender<T>) -> Self {
        let chan = Chan(inner, PhantomData);
        Sender {
            pollify: Pollify::new(chan),
            used_in_poll_ready: PhantomData,
        }
    }

    fn pin_project_inner(self: Pin<&mut Self>) -> Pin<&mut Pollify<Chan<'a, T>>> {
        unsafe { self.map_unchecked_mut(|this| &mut this.pollify) }
    }

    /// Returns sender readiness state
    pub fn is_ready(&self) -> bool {
        self.pollify.is_ready()
    }

    /// Sends a message.
    ///
    /// This method panics if the `Sender` is not ready.
    pub fn send(self: Pin<&mut Self>, value: T) {
        if !self.pollify.is_ready() {
            panic!("called send() on non-ready Sender")
        }
        let permit = self
            .pin_project_inner()
            .extract()
            .expect("poll_ready would handle Err");
        permit.send(value);
    }

    /// Disarm permit. This releases the reserved slot in the bounded channel.
    /// If acquire was in progress, it is aborted.
    ///
    /// Returns true if before the call this sender owned a permit.
    pub fn disarm(self: Pin<&mut Self>) -> bool {
        self.pin_project_inner().cancel()
    }

    /// Tries to acquire a permit.
    ///
    /// This function can not be called when the `Sender` is ready.
    pub fn poll_ready(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), SendError<()>>> {
        let mut inner = self.pin_project_inner();
        match inner.as_mut().poll(cx) {
            Poll::Ready(()) => {
                if inner.as_mut().output_ref().is_ok() {
                    Poll::Ready(Ok(()))
                } else {
                    let output = inner.extract();
                    Poll::Ready(Err(output.unwrap_err()))
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod _props {
    use super::*;
    fn _verify_not_unpin<U: Send>(x: Sender<'_, U>) {
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

    fn _verify_send<U: Send>(x: Sender<'_, U>) {
        fn inner(_x: impl Send) {}
        inner(x)
    }
    fn _verify_sync<U: Send + Sync>(x: Sender<'_, U>) {
        fn inner(_x: impl Sync) {}
        inner(x)
    }
}
